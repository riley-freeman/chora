use crate::coordination::Transform;
use crate::instanced_render::WeakInstancedRender;
use crate::mesh::{Mesh, WeakModel};
use cgmath::{Matrix4, Quaternion, Rad, Rotation3, Vector3};
use wgpu::Device;
use wgpu::wgt::BufferDescriptor;
use wgpu::{Buffer, BufferUsages, Queue};

use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, Weak};
use std::{mem, usize};

pub(crate) struct ModelInner {
    pub(crate) mutable: bool,
    pub(crate) meshes: Vec<Mesh>,

    pub(crate) transform: *const Transform,

    pub(crate) model_buffers: Vec<Buffer>,
    pub(crate) model_buffer_info: Mutex<ModelBufferStruct>,
    pub(crate) model_buffer_update: AtomicUsize,

    pub(crate) mesh_instanced_renders: Mutex<Vec<WeakInstancedRender>>,
    pub(crate) mesh_buffer_offsets: Vec<AtomicUsize>,
}

#[derive(Clone)]
pub struct Model {
    pub(crate) inner: Arc<ModelInner>,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub(crate) struct ModelBufferStruct {
    pub(crate) model_matrix: Matrix4<f32>,
}

// Safety: ModelBufferStruct is repr(C) and contains only Matrix4<f32> which is Pod
unsafe impl bytemuck::Pod for ModelBufferStruct {}
unsafe impl bytemuck::Zeroable for ModelBufferStruct {}

impl Default for ModelBufferStruct {
    fn default() -> Self {
        ModelBufferStruct {
            model_matrix: Matrix4::from_scale(1.0),
        }
    }
}

impl Model {
    pub fn new(
        device: &Device,
        queue: &Queue,
        meshes: Vec<Mesh>,
        mutable: bool,
        transform: &Transform,
        buffers: usize,
    ) -> Self {
        let buffer_info = unsafe {
            ModelBufferStruct {
                model_matrix: Self::compute_matrix(&*(transform as *const Transform)),
            }
        };
        let model_buffer_info = Mutex::new(buffer_info);

        // Create a buffer for the model's render data
        let mut model_buffers = vec![];
        for i in 0..buffers {
            let model_buffer = device.create_buffer(&BufferDescriptor {
                size: mem::size_of::<ModelBufferStruct>() as _,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
                label: Some("Model Buffer"),
            });
            queue.write_buffer(&model_buffer, 0, bytemuck::cast_slice(&[buffer_info]));
            model_buffers.push(model_buffer);
        }

        let buffer_offsets = meshes.iter().map(|_| AtomicUsize::new(0)).collect();
        let instanced_renders = meshes
            .iter()
            .map(|_| WeakInstancedRender::default())
            .collect();

        let inner = ModelInner {
            mutable,
            meshes,

            transform: transform as _,

            model_buffers,
            model_buffer_update: AtomicUsize::new(usize::MAX),
            model_buffer_info,
            mesh_buffer_offsets: buffer_offsets,
            mesh_instanced_renders: Mutex::new(instanced_renders),
        };

        let model = Self {
            inner: Arc::new(inner),
        };

        // Set parent model reference in each mesh
        let weak_model = model.downgrade();
        for mesh in &model.inner.meshes {
            mesh.set_parent_model(weak_model.clone());
        }

        model
    }

    /// Compute a model matrix from position, rotation (Euler angles in radians), and scale
    fn compute_matrix(transform: &Transform) -> Matrix4<f32> {
        // Create translation matrix
        let translation = Matrix4::from_translation(transform.position());

        // Create rotation matrix
        let rotation_matrix = Matrix4::from(transform.rotation());

        // Create scale matrix
        let scale = transform.scale();
        let scale_matrix = Matrix4::from_nonuniform_scale(scale.x, scale.y, scale.z);

        transform
            .updated
            .store(false, std::sync::atomic::Ordering::Relaxed);

        // Combine: translation * rotation * scale (TRS order)
        translation * rotation_matrix * scale_matrix
    }

    /// Get the current model matrix
    pub fn model_matrix(&self) -> Matrix4<f32> {
        self.inner.model_buffer_info.lock().unwrap().model_matrix
    }

    /// Update the model matrix from the current position/rotation/scale values
    ///
    /// Call this after modifying the position, rotation, or scale vectors
    pub fn update_matrix(&self) -> bool {
        unsafe {
            let transform = &*self.inner.transform;
            if transform.updated.load(std::sync::atomic::Ordering::Relaxed) == true {
                let new_matrix = Self::compute_matrix(transform);
                self.inner.model_buffer_info.lock().unwrap().model_matrix = new_matrix;
                true // Data bas changed
            } else {
                false // Data has remained the same
            }
        }
    }

    /// Update the model matrix and upload it to the GPU buffer
    ///
    /// Returns true if a buffer has updated.
    ///
    /// # Arguments
    /// * `queue` - The GPU queue for uploading data
    pub fn update_matrix_and_upload(&self, device: &Device, queue: &Queue, buffer: usize) -> bool {
        let changed = self.update_matrix();
        // You know... I really dont care about it not actually being atomic... at least not yet...
        let old = self
            .inner
            .model_buffer_update
            .load(std::sync::atomic::Ordering::Relaxed);
        if changed {
            self.inner
                .model_buffer_update
                .store(1 << buffer, std::sync::atomic::Ordering::Relaxed);
        } else {
            // Check if this buffer has already been updated
            // if so, dont do any more work
            if old & (1 << buffer) != 0 {
                return false;
            }

            self.inner
                .model_buffer_update
                .store(old | 1 << buffer, std::sync::atomic::Ordering::Relaxed);
        }

        let matrix = self.model_matrix();
        let buffer_data = ModelBufferStruct {
            model_matrix: matrix,
        };

        let ir_pairs: Vec<(crate::instanced_render::InstancedRender, usize)> = {
            let ir_lock = self.inner.mesh_instanced_renders.lock().unwrap();
            ir_lock.iter().zip(&self.inner.mesh_buffer_offsets).filter_map(|(ir, offset)| {
                Some((ir.upgrade()?, offset.load(std::sync::atomic::Ordering::Relaxed)))
            }).collect()
        };
        for (ir, instance_offset) in ir_pairs {
            ir.update_instance_buffer(device, queue, buffer, instance_offset, &[matrix]);
        }

        queue.write_buffer(
            &self.inner.model_buffers[buffer],
            0,
            bytemuck::cast_slice(&[buffer_data]),
        );
        true
    }

    /// Get a reference to the model's GPU buffer containing the model matrix
    pub fn model_buffer(&self, variant: usize) -> &Buffer {
        &self.inner.model_buffers[variant]
    }

    /// Get the meshes in this model
    pub fn meshes(&self) -> &[Mesh] {
        &self.inner.meshes
    }

    /// Check if this model is mutable (allows matrix updates)
    pub fn is_mutable(&self) -> bool {
        self.inner.mutable
    }

    /// Record which instanced render group owns mesh `mesh_index` and what slot it occupies.
    pub(crate) fn set_instanced_render(
        &self,
        mesh_index: usize,
        ir: crate::instanced_render::WeakInstancedRender,
        instance_offset: usize,
    ) {
        if mesh_index < self.inner.mesh_buffer_offsets.len() {
            self.inner.mesh_buffer_offsets[mesh_index]
                .store(instance_offset, std::sync::atomic::Ordering::Relaxed);
        }
        let mut lock = self.inner.mesh_instanced_renders.lock().unwrap();
        if mesh_index < lock.len() {
            lock[mesh_index] = ir;
        }
    }

    /// Create a weak reference to this model
    pub(crate) fn downgrade(&self) -> WeakModel {
        WeakModel(Arc::downgrade(&self.inner))
    }
}

impl<'a> IntoIterator for &'a Model {
    type Item = &'a Mesh;
    type IntoIter = MeshIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        MeshIter {
            meshes: &self.inner.meshes,
            index: 0,
        }
    }
}

pub struct MeshIter<'a> {
    meshes: &'a [Mesh],
    index: usize,
}

impl<'a> Iterator for MeshIter<'a> {
    type Item = &'a Mesh;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.meshes.len() {
            let mesh = &self.meshes[self.index];
            self.index += 1;
            Some(mesh)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Renderer;
    use crate::render_pipeline::RenderPipelineFlags;
    use cgmath::Euler;
    use std::path::Path;
    use wgpu::AddressMode;
    use wgpu::FilterMode;

    #[test]
    fn test_model_matrix_computation() {
        let renderer = Renderer::new(512, 512, 2).unwrap();

        // Create a simple shader and mesh
        let shader = include_str!("shaders/bs_shader.wgsl");
        let texture = renderer
            .load_texture_from_path(Path::new("kenya.jpg"))
            .unwrap();
        let textures = Vec::from([texture]);
        let sampler = renderer.create_sampler(AddressMode::Repeat, FilterMode::Linear);
        let render_pipeline = renderer.create_render_pipeline(
            shader,
            &textures,
            Some(sampler),
            RenderPipelineFlags::OVERRIDE_VERTEX_INPUT,
        );

        let vertices = [[0.0f32; 16]; 3];
        let vertices = vertices
            .iter()
            .flat_map(|v| v.iter())
            .copied()
            .collect::<Vec<f32>>();
        let indices = [0, 1, 2];

        let mesh = renderer
            .create_mesh(&vertices, &indices, render_pipeline)
            .unwrap();

        // Create model with transformation
        let transform = Transform::new(
            Vector3::new(1.0, 2.0, 3.0),
            Quaternion::from(Euler::new(
                Rad(0.0),
                Rad(std::f32::consts::PI / 4.0),
                Rad(0.0),
            )),
            Vector3::new(2.0, 2.0, 2.0),
        );

        let model = renderer.create_model(vec![mesh], true, &transform).unwrap();

        // Get the model matrix
        let matrix = model.model_matrix();

        println!("Model Matrix:\n{:?}", matrix);

        // Verify matrix is not identity
        assert_ne!(matrix, Matrix4::from_scale(1.0));

        // Verify mesh can access parent model's matrix
        for mesh in model.meshes() {
            let mesh_matrix = mesh.model_matrix();
            assert!(
                mesh_matrix.is_some(),
                "Mesh should have access to model matrix"
            );
            assert_eq!(
                mesh_matrix.unwrap(),
                matrix,
                "Mesh matrix should match model matrix"
            );

            // Verify mesh can access model buffer
            let buffer = mesh.model_buffer(0);
            assert!(buffer.is_some(), "Mesh should have access to model buffer");
        }

        println!("✓ Model matrix system working correctly!");
    }

    #[test]
    fn test_model_matrix_update() {
        let renderer = Renderer::new(512, 512, 2).unwrap();

        let shader = include_str!("shaders/bs_shader.wgsl");
        let texture = renderer
            .load_texture_from_path(Path::new("kenya.jpg"))
            .unwrap();
        let sampler = renderer.create_sampler(AddressMode::Repeat, FilterMode::Linear);
        let render_pipeline = renderer.create_render_pipeline(
            shader,
            &vec![texture],
            Some(sampler),
            RenderPipelineFlags::OVERRIDE_VERTEX_INPUT,
        );

        let vertices = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let indices = [0, 1, 2];
        let mesh = renderer
            .create_mesh(&vertices, &indices, render_pipeline)
            .unwrap();

        let mut transform = Transform::default();
        let model = renderer.create_model(vec![mesh], true, &transform).unwrap();

        let initial_matrix = model.model_matrix();

        // Modify position
        transform.set_position_x(5.0);
        model.update_matrix();

        let updated_matrix = model.model_matrix();

        // Matrix should have changed
        assert_ne!(initial_matrix, updated_matrix);

        println!("Initial Matrix:\n{:?}", initial_matrix);
        println!("Updated Matrix:\n{:?}", updated_matrix);
        println!("✓ Model matrix update working correctly!");
    }
}
