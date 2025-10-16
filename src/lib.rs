use crate::camera::Camera;
use crate::instanced_render::InstancedRender;
use crate::linked_list::LinkedList;
use crate::mesh::{Mesh, WeakMesh};
use crate::model::Model;
use crate::render_pipeline::{RenderPipeline, WeakRenderPipeline};
use crate::sampler::Sampler;
use crate::texture::{Spritesheet, Texture};
use cgmath::Vector3;
use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::c_void;
use std::io;
use std::ops::DerefMut;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, LazyLock, Mutex};
use wgpu::BackendOptions;
use wgpu::Backends;
use wgpu::Device;
use wgpu::Instance;
use wgpu::InstanceDescriptor;
use wgpu::InstanceFlags;
use wgpu::MemoryBudgetThresholds;
use wgpu::Queue;
use wgpu::RequestAdapterOptions;
use wgpu::wgt::TextureFormat;
use wgpu::wgt::{DeviceDescriptor, SamplerDescriptor};
use wgpu::{
    Adapter, AddressMode, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingType, ColorTargetState, ColorWrites, FilterMode, FragmentState,
    MultisampleState, PipelineCompilationOptions, PipelineLayoutDescriptor,
    PrimitiveState, RenderPipelineDescriptor, SamplerBindingType, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, TextureSampleType, TextureViewDimension, VertexState,
};

pub mod camera;
pub mod error;
mod instanced_render;
mod linked_list;
pub mod mesh;
pub mod model;
pub mod render_pipeline;
pub mod render_target;
pub mod sampler;
pub mod texture;

const MAX_TEXTURE_SIZE: u32 = 2048;
const _MAX_BINDABLE_TEXTURE_COUNT: usize = 16;

static INSTANCE: LazyLock<Instance> = LazyLock::new(|| {
    Instance::new(&InstanceDescriptor {
        backends: Backends::PRIMARY,
        #[cfg(debug_assertions)]
        flags: InstanceFlags::DEBUG,
        #[cfg(not(debug_assertions))]
        flags: InstanceFlags::empty(),
        memory_budget_thresholds: MemoryBudgetThresholds {
            for_resource_creation: None,
            for_device_loss: None,
        },
        backend_options: BackendOptions {
            ..Default::default()
        },
    })
});

struct RendererInner {
    _adapter: Adapter,
    device: Device,
    queue: Queue,
    buffers: usize,

    cast_render_pipeline: wgpu::RenderPipeline,
    cast_bind_group_layout: BindGroupLayout,
    cast_sampler: wgpu::Sampler,

    // Final / Main output
    camera: Camera,
    _position: Box<Vector3<f32>>,
    _pitch: Box<f32>,
    _yaw: Box<f32>,
    _roll: Box<f32>,

    // Mesh Database (I think I don't really know what this is called)
    mesh_collection: HashMap<*const c_void, LinkedList<WeakMesh>>,

    mesh_instanced_renders: HashMap<*const c_void, InstancedRender>,
    independent_renders: HashMap<*const c_void, WeakRenderPipeline>,
}

unsafe impl Sync for RendererInner {}
unsafe impl Send for RendererInner {}

impl RendererInner {}

#[derive(Clone)]
pub struct Renderer(Arc<Mutex<RendererInner>>);

impl Renderer {
    pub fn new(width: u32, height: u32, buffers: usize) -> Result<Self, error::ChoraError> {
        let adapter = pollster::block_on(INSTANCE.request_adapter(&RequestAdapterOptions {
            ..Default::default()
        }))
        .map_err(|_| error::ChoraError::FailedToFindAdapter {})?;

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: Some("0x99 CRAYON CHORA"),
            ..Default::default()
        }))
        .map_err(|_| error::ChoraError::FailedGettingSuitableDevice {})?;

        let position = Box::new(Vector3::new(0.0f32, 0.0f32, 0.0f32));
        let pitch = Box::new(0.0f32);
        let yaw = Box::new(0.0f32);
        let roll = Box::new(0.0f32);
        let fov = 77.0f32;

        let camera = Camera::new(
            &device,
            width,
            height,
            buffers,
            false,
            fov,
            false,
            position.as_ref().as_ref(),
            &pitch,
            &yaw,
            &roll,
        )?;

        // Create a bind group layout and render pipeline

        let cast_bind_group_layout = Self::create_cast_bind_group_layout(&device);

        let cast_render_pipeline =
            Self::create_cast_render_pipeline(&device, &cast_bind_group_layout);

        let cast_sampler = device.create_sampler(&SamplerDescriptor::default());

        let inner = RendererInner {
            _adapter: adapter,
            device,
            queue,
            buffers,

            camera,
            _position: position,
            _pitch: pitch,
            _yaw: yaw,
            _roll: roll,

            cast_bind_group_layout,
            cast_render_pipeline,
            cast_sampler,

            mesh_collection: Default::default(),

            independent_renders: Default::default(),
            mesh_instanced_renders: Default::default(),
        };

        let result = Renderer(Arc::new(Mutex::new(inner)));

        Ok(result)
    }

    fn create_cast_render_pipeline(
        device: &Device,
        cast_bind_group_layout: &BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let cast_shader = include_str!("./shaders/cast.wgsl");
        let shader_code = Cow::from(cast_shader);

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&cast_bind_group_layout],
            push_constant_ranges: &[],
        });

        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: ShaderSource::Wgsl(shader_code),
        });

        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    write_mask: ColorWrites::all(),
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                })],
            }),
            primitive: PrimitiveState::default(),
            multisample: MultisampleState::default(),
            cache: None,
            depth_stencil: None,
            multiview: None,
        })
    }

    fn create_cast_bind_group_layout(device: &Device) -> BindGroupLayout {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    count: None,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                    },
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    count: None,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                },
            ],
        })
    }

    pub fn create_camera(
        &self,
        width: u32,
        height: u32,
        hdr: bool,
        fov: f32,
        orthographic: bool,
        position: &[f32; 3],
        pitch: &f32,
        yaw: &f32,
        roll: &f32,
    ) -> Result<Camera, error::ChoraError> {
        let this = self.0.lock().unwrap();

        Camera::new(
            &this.device,
            width,
            height,
            this.buffers,
            hdr,
            fov,
            orthographic,
            position,
            pitch,
            yaw,
            roll,
        )
    }

    pub fn create_mesh(
        &self,
        vertices: &[f32],
        indices: &[i32],
        render_pipeline: RenderPipeline,
    ) -> Result<Mesh, error::ChoraError> {
        let device = self.0.lock().unwrap().device.clone();
        Ok(Mesh::new(
            self.clone(),
            &device,
            vertices,
            indices,
            render_pipeline,
        ))
    }

    pub fn create_model(
        &self,
        meshes: Vec<Mesh>,
        mutable: bool,
        position: &Vector3<f32>,
        rotation: &Vector3<f32>,
        scale: &Vector3<f32>,
    ) -> Result<Model, error::ChoraError> {
        let device = self.0.lock().unwrap().device.clone();

        Ok(Model::new(
            &device,
            meshes,
            mutable,
            position as *const _ as _,
            rotation as *const _ as _,
            scale as *const _ as _,
        ))
    }

    pub fn create_texture(
        &self,
        width: u32,
        height: u32,
        format: TextureFormat,
        data: Option<&[u8]>,
    ) -> Texture {
        Texture::new_locked(
            self.clone(),
            &self.0.lock().unwrap(),
            width,
            height,
            format,
            data,
        )
    }

    pub fn create_spritesheet(
        &self,
    ) -> Spritesheet {
        let lock = self.0.lock().unwrap();
        let device = lock.device.clone();
        let queue = lock.queue.clone();
        let cast_layout = lock.cast_bind_group_layout.clone();
        let cast_render_pipeline = lock.cast_render_pipeline.clone();
        let cast_sampler = lock.cast_sampler.clone();
        drop(lock);

        Spritesheet::new(
            self.clone(),
            &device,
            &queue,
            &cast_layout,
            &cast_render_pipeline,
            &cast_sampler,
        )
    }

    pub fn load_texture_from_path(&self, path: &Path) -> io::Result<Texture> {
        Texture::load_from_file_locked(self.clone(), &self.0.lock().unwrap(), path)
    }

    pub fn create_sampler(&self, address_mode: AddressMode, filter_mode: FilterMode) -> Sampler {
        let device = self.0.lock().unwrap().device.clone();
        Sampler::new(self.clone(), &device, address_mode, filter_mode)
    }

    pub fn create_render_pipeline(
        &self,
        code: &str,
        textures: &Vec<Texture>,
        sampler: Option<Sampler>,
        allow_world_uniform: bool,
        allow_camera_uniform: bool,
        allow_object_uniform: bool,
    ) -> RenderPipeline {
        let lock = self.0.lock().unwrap();
        RenderPipeline::new(
            &lock.device,
            &lock.camera,
            code,
            textures,
            sampler,
            allow_world_uniform,
            allow_camera_uniform,
            allow_object_uniform,
        )
    }

    pub fn add_to_render_queue(&mut self, model: Model) -> Result<(), error::ChoraError> {
        for mesh in model.into_iter() {
            self.add_mesh_to_render_queue(&mesh)?;
            mesh.added.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn add_mesh_to_render_queue(&mut self, mesh: &Mesh) -> Result<(), error::ChoraError> {
        let mesh_address = mesh.inner.as_ref() as *const _ as *const c_void;
        let weak_mesh = mesh.downgrade();

        let render_pipeline = mesh.render_pipeline();

        let clone = self.clone();
        let mut this = self.0.lock().unwrap();
        let this_mut = unsafe {
            std::mem::transmute_copy::<&mut RendererInner, &mut RendererInner>(&this.deref_mut())
        };

        // Organize the mesh into groups
        let mesh_collection = this
            .mesh_collection
            .entry(mesh_address)
            .or_insert(LinkedList::new());

        let _mesh_collection_node = mesh_collection.push_front(weak_mesh.clone());

        // Check for instance meshes
        let mesh_collection = this.mesh_collection.get(&mesh_address).unwrap();

        let mesh_collection_len = mesh_collection.len();
        if mesh_collection_len > 1 {
            let instanced_render = this_mut
                .mesh_instanced_renders
                .entry(mesh_address)
                .or_insert(InstancedRender::new(clone, &this, mesh));
            instanced_render.add_mesh(&this, &mesh);

            // Double up mesh
            if mesh_collection_len == 2 {
                instanced_render.add_mesh(&this, &mesh);
                this.independent_renders.remove(&mesh_address).unwrap();
            }
        } else {
            // Create a single independent render.
            this.independent_renders
                .insert(mesh_address, render_pipeline.downgrade());
        }

        Ok(())
    }

    pub(crate) fn remove_mesh_from_render_queue(&self, mesh: &Mesh) {
        let render_pipeline = mesh.render_pipeline();

        let mesh_address = mesh.inner.as_ref() as *const _ as *const c_void;

        let mut this = self.0.lock().unwrap();

        this.independent_renders.remove(&mesh_address);
        let mut create_independent_render = false;
        if let Some(instanced_renders) = this.mesh_instanced_renders.get_mut(&mesh_address) {
            instanced_renders.remove_mesh(mesh);
            if instanced_renders.mesh_count() == 0 {
                this.mesh_instanced_renders.remove(&mesh_address);
                create_independent_render = true;
            }
        }

        if create_independent_render {
            this.independent_renders
                .insert(mesh_address, render_pipeline.downgrade());
        }
    }

    pub fn device(&self) -> Device {
        self.0.lock().unwrap().device.clone()
    }

    pub fn queue(&self) -> Queue {
        self.0.lock().unwrap().queue.clone()
    }

    pub fn main_camera(&self) -> Camera {
        self.0.lock().unwrap().camera.clone()
    }

    pub fn cast_bind_group_layout(&self) -> BindGroupLayout {
        self.0.lock().unwrap().cast_bind_group_layout.clone()
    }

    pub fn cast_sampler(&self) -> wgpu::Sampler {
        self.0.lock().unwrap().cast_sampler.clone()
    }
    pub fn cast_render_pipeline(&self) -> wgpu::RenderPipeline {
        self.0.lock().unwrap().cast_render_pipeline.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::drop;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn hello_world() {
        println!("Hello, World!");
    }

    #[test]
    fn new_renderer() {
        let _renderer = Renderer::new(512, 512, 2).unwrap();
    }

    /// Tests the mesh grouping functionality of the camera
    ///
    /// This test verifies that:
    /// 1. Single meshes are added as independent renders
    /// 2. Multiple identical meshes are grouped into instanced renders
    /// 3. Different meshes remain as separate render groups
    /// 4. Mesh collection tracking works correctly
    #[test]
    pub fn independent_instanced_grouping_test() {
        let mut renderer = Renderer::new(512, 512, 2).unwrap();

        let shader = include_str!("shaders/bs_shader.wgsl");

        let texture = renderer
            .load_texture_from_path(Path::new("kenya.jpg"))
            .unwrap();
        let textures = Vec::from([texture]);

        let sampler = renderer.create_sampler(AddressMode::Repeat, FilterMode::Linear);

        let render_pipeline =
            renderer.create_render_pipeline(shader, &textures, Some(sampler), false, false, false);

        // Create test meshes
        let vertices = [[0.5, 0.5, 0.0], [-0.5, 0.5, 0.0], [-0.0, -0.5, 0.0]];
        let vertices = vertices
            .iter()
            .flat_map(|v| v.iter())
            .copied()
            .collect::<Vec<f32>>();
        let indices = [0, 1, 2];

        let i_triangle0 = renderer
            .create_mesh(&vertices, &indices, render_pipeline.clone())
            .unwrap();
        let i_triangle1 = Mesh {
            inner: Arc::clone(&i_triangle0.inner),
            added: AtomicBool::new(false),
        };

        let s_triangle2 = renderer
            .create_mesh(&vertices, &indices, render_pipeline.clone())
            .unwrap();

        // Test single mesh (should be independent)
        renderer.add_mesh_to_render_queue(&i_triangle0).unwrap();
        {
            let lock = renderer.0.lock().unwrap();
            assert_eq!(
                lock.independent_renders.len(),
                1,
                "Single mesh should be independent"
            );
            assert_eq!(
                lock.mesh_instanced_renders.len(),
                0,
                "No instanced renders should exist"
            );
            assert_eq!(
                lock.mesh_collection.len(),
                1,
                "Should have one mesh collection"
            );
            assert_eq!(
                lock.mesh_collection.values().next().unwrap().len(),
                1,
                "Collection should have one mesh"
            );
        }

        // Test identical mesh (should become instanced)
        renderer.add_mesh_to_render_queue(&i_triangle1).unwrap();
        {
            let lock = renderer.0.lock().unwrap();
            assert_eq!(
                lock.independent_renders.len(),
                0,
                "No independent renders should remain"
            );
            assert_eq!(
                lock.mesh_instanced_renders.len(),
                1,
                "Should have one instanced render"
            );
            assert_eq!(
                lock.mesh_collection.len(),
                1,
                "Should have one mesh collection"
            );
            assert_eq!(
                lock.mesh_collection.values().next().unwrap().len(),
                2,
                "Collection should have two meshes"
            );

            let i_render = lock.mesh_instanced_renders.iter().nth(0).unwrap();
            let count = i_render.1.mesh_count();
            assert_eq!(count, 2, "Instance count should be 2");
        }

        // Test different mesh (should be independent)
        renderer.add_mesh_to_render_queue(&s_triangle2).unwrap();
        {
            let lock = renderer.0.lock().unwrap();
            assert_eq!(
                lock.independent_renders.len(),
                1,
                "Should have one independent render"
            );
            assert_eq!(
                lock.mesh_instanced_renders.len(),
                1,
                "Should have one instanced render"
            );
            assert_eq!(
                lock.mesh_collection.len(),
                2,
                "Should have two mesh collections"
            );

            let i_render = lock.mesh_instanced_renders.iter().nth(0).unwrap();
            let count = i_render.1.mesh_count();
            assert_eq!(count, 2, "Instance count should remain 2");
        }

        // Clean up
        drop(i_triangle0);
        drop(i_triangle1);
        drop(s_triangle2);
    }
}
