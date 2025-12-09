use cgmath::{Matrix4, Vector3, Rad, Quaternion, Rotation3};
use wgpu::{BindGroup, BindGroupEntry, BindGroupLayout, BindingResource, Buffer, BufferBinding, BufferUsages, Queue};
use wgpu::Device;
use wgpu::wgt::BufferDescriptor;
use crate::mesh::{Mesh, WeakModel};

use std::mem;
use std::sync::{Arc, Mutex};

pub(crate) struct ModelInner {
    pub(crate) mutable: bool,
    pub(crate) meshes: Vec<Mesh>,

    pub(crate) position: *const Vector3<f32>,
    pub(crate) rotation: *const Vector3<f32>,
    pub(crate) scale: *const Vector3<f32>,

    pub(crate) model_buffer: Buffer,
    pub(crate) model_buffer_info: Mutex<ModelBufferStruct>,
}

pub struct Model {
    inner: Arc<ModelInner>,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub(crate) struct ModelBufferStruct {
    pub(crate) model_matrix: Matrix4<f32>
}

// Safety: ModelBufferStruct is repr(C) and contains only Matrix4<f32> which is Pod
unsafe impl bytemuck::Pod for ModelBufferStruct {}
unsafe impl bytemuck::Zeroable for ModelBufferStruct {}

impl Default for ModelBufferStruct {
    fn default() -> Self {
        ModelBufferStruct { model_matrix: Matrix4::from_scale(1.0) }
    }
}

impl Model {
    pub fn new(device: &Device, meshes: Vec<Mesh>, mutable: bool, bind_group_layout: BindGroupLayout, world_buffer: Option<Buffer>, camera_buffer: Option<Buffer>,
               position: *const f32, rotation: *const f32, scale: *const f32) -> Self
    {
        // Create a buffer for the model's render data
        let model_buffer = device.create_buffer(&BufferDescriptor {
            size: mem::size_of::<ModelBufferStruct>() as _,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
            label: Some("Model Buffer"),
        });

        let model_buffer_info = Mutex::new(unsafe {
            ModelBufferStruct {
                model_matrix: Self::compute_matrix(
                    &*(position as *const Vector3<f32>),
                    &*(rotation as *const Vector3<f32>),
                    &*(scale as *const Vector3<f32>),
                )
            }
        });
        


        let inner = ModelInner {
            mutable,
            meshes,

            position: position as _,
            rotation: rotation as _,
            scale: scale as _,

            model_buffer,
            model_buffer_info,
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
    fn compute_matrix(position: &Vector3<f32>, rotation: &Vector3<f32>, scale: &Vector3<f32>) -> Matrix4<f32> {
        // Create translation matrix
        let translation = Matrix4::from_translation(*position);

        // Create rotation matrix from Euler angles (pitch, yaw, roll)
        // Apply rotations in order: Z (roll) -> X (pitch) -> Y (yaw)
        let rotation_x = Quaternion::from_angle_x(Rad(rotation.x));
        let rotation_y = Quaternion::from_angle_y(Rad(rotation.y));
        let rotation_z = Quaternion::from_angle_z(Rad(rotation.z));
        let rotation_quat = rotation_y * rotation_x * rotation_z;
        let rotation_matrix = Matrix4::from(rotation_quat);

        // Create scale matrix
        let scale_matrix = Matrix4::from_nonuniform_scale(scale.x, scale.y, scale.z);

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
    pub fn update_matrix(&self) {
        unsafe {
            let position = &*self.inner.position;
            let rotation = &*self.inner.rotation;
            let scale = &*self.inner.scale;

            let new_matrix = Self::compute_matrix(position, rotation, scale);
            self.inner.model_buffer_info.lock().unwrap().model_matrix = new_matrix;
        }
    }

    /// Update the model matrix and upload it to the GPU buffer
    ///
    /// # Arguments
    /// * `queue` - The GPU queue for uploading data
    pub fn update_matrix_and_upload(&self, queue: &Queue) {
        self.update_matrix();
        let matrix = self.model_matrix();

        let buffer_data = ModelBufferStruct {
            model_matrix: matrix,
        };

        // Upload to GPU
        queue.write_buffer(&self.inner.model_buffer, 0, bytemuck::cast_slice(&[buffer_data]));
    }

    /// Get a reference to the model's GPU buffer containing the model matrix
    pub fn model_buffer(&self) -> &Buffer {
        &self.inner.model_buffer
    }

    /// Get the meshes in this model
    pub fn meshes(&self) -> &[Mesh] {
        &self.inner.meshes
    }

    /// Check if this model is mutable (allows matrix updates)
    pub fn is_mutable(&self) -> bool {
        self.inner.mutable
    }

    /// Get raw pointers to position/rotation/scale (for advanced use)
    pub fn transform_ptrs(&self) -> (*const Vector3<f32>, *const Vector3<f32>, *const Vector3<f32>) {
        (self.inner.position, self.inner.rotation, self.inner.scale)
    }

    /// Create a weak reference to this model
    pub(crate) fn downgrade(&self) -> WeakModel {
        WeakModel(Arc::downgrade(&self.inner))
    }
}

fn create_buffer_group_entry<'a>(buffer: &Buffer, binding: u32) -> BindGroupEntry<'_> {
    BindGroupEntry {
        binding: binding,
        resource: BindingResource::Buffer(BufferBinding {
            buffer: buffer,
            offset: 0,
            size: None
        })
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
    use wgpu::AddressMode;
    use wgpu::FilterMode;
    use std::path::Path;

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
            RenderPipelineFlags::empty(),
        );

        let vertices = [[0.5, 0.5, 0.0], [-0.5, 0.5, 0.0], [-0.0, -0.5, 0.0]];
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
        let position = Vector3::new(1.0, 2.0, 3.0);
        let rotation = Vector3::new(0.0, std::f32::consts::PI / 4.0, 0.0); // 45 degrees around Y
        let scale = Vector3::new(2.0, 2.0, 2.0);

        let model = renderer
            .create_model(
                vec![mesh],
                true,
                &position,
                &rotation,
                &scale,
            )
            .unwrap();

        // Get the model matrix
        let matrix = model.model_matrix();

        println!("Model Matrix:\n{:?}", matrix);

        // Verify matrix is not identity
        assert_ne!(matrix, Matrix4::from_scale(1.0));

        // Verify mesh can access parent model's matrix
        for mesh in model.meshes() {
            let mesh_matrix = mesh.model_matrix();
            assert!(mesh_matrix.is_some(), "Mesh should have access to model matrix");
            assert_eq!(mesh_matrix.unwrap(), matrix, "Mesh matrix should match model matrix");

            // Verify mesh can access model buffer
            let buffer = mesh.model_buffer();
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
            RenderPipelineFlags::empty(),
        );

        let vertices = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let indices = [0, 1, 2];
        let mesh = renderer.create_mesh(&vertices, &indices, render_pipeline).unwrap();

        let mut position = Vector3::new(0.0, 0.0, 0.0);
        let rotation = Vector3::new(0.0, 0.0, 0.0);
        let scale = Vector3::new(1.0, 1.0, 1.0);

        let model = renderer
            .create_model(vec![mesh], true, &position, &rotation, &scale)
            .unwrap();

        let initial_matrix = model.model_matrix();

        // Modify position
        position.x = 5.0;
        model.update_matrix();

        let updated_matrix = model.model_matrix();

        // Matrix should have changed
        assert_ne!(initial_matrix, updated_matrix);

        println!("Initial Matrix:\n{:?}", initial_matrix);
        println!("Updated Matrix:\n{:?}", updated_matrix);
        println!("✓ Model matrix update working correctly!");
    }
}
