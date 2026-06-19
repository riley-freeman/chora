use crate::camera::{Camera, CameraBufferStruct};
use crate::coordination::Transform;
use crate::error::ChoraError;
use crate::instanced_render::InstancedRender;
use crate::linked_list::LinkedList;
use crate::mesh::{Mesh, WeakMesh};
use crate::model::Model;
use crate::render_pipeline::{RenderPipeline, RenderPipelineFlags, WeakRenderPipeline};
use crate::sampler::Sampler;
use crate::texture::{Spritesheet, Texture};
use cgmath::Vector3;
use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::c_void;
use std::io;
use std::mem::{self, replace};
use std::ops::DerefMut;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, LazyLock, Mutex, Weak};
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
    BindingType, ColorTargetState, ColorWrites, FilterMode, FragmentState, MultisampleState,
    PipelineCompilationOptions, PipelineLayoutDescriptor, PresentMode, PrimitiveState,
    RenderPipelineDescriptor, SamplerBindingType, ShaderModuleDescriptor, ShaderSource,
    ShaderStages, Surface, SurfaceConfiguration, TextureSampleType, TextureViewDimension,
    VertexState,
};
use wgpu::{BackendOptions, SubmissionIndex};

pub mod camera;
pub mod coordination;
pub mod error;
mod instanced_render;
mod linked_list;
pub mod mesh;
pub mod model;
pub mod render_pipeline;
pub mod render_target;
pub mod sampler;
mod shader_merger;
mod shader_parser;
mod shader_rewriter;
pub mod texture;
mod utils;

const MAX_FRAMES_IN_FLIGHT: usize = 3;
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

    cast_bind_group_layout: BindGroupLayout,
    // Offscreen blit pipeline — always Rgba8Unorm, used by the atlas/spritesheet.
    cast_render_pipeline: wgpu::RenderPipeline,
    // Surface blit pipeline — matches swapchain format; set after create_surface().
    cast_render_pipeline_surface: Option<wgpu::RenderPipeline>,
    cast_sampler: wgpu::Sampler,

    camera_bind_group_layout: BindGroupLayout,

    // Swapchain support (optional)
    surface: Option<Surface<'static>>,
    surface_config: Option<SurfaceConfiguration>,

    submission_indices: [Option<SubmissionIndex>; MAX_FRAMES_IN_FLIGHT],
    submission_id: usize,

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

#[derive(Clone)]
pub struct Renderer(Arc<Mutex<RendererInner>>);

#[derive(Clone)]
pub struct WeakRenderer(Weak<Mutex<RendererInner>>);

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

        // Create a bind group layout and render pipeline
        let cast_bind_group_layout = Self::create_cast_bind_group_layout(&device);

        let cast_render_pipeline = Self::create_cast_render_pipeline(
            &device,
            &cast_bind_group_layout,
            TextureFormat::Rgba8Unorm,
        );

        let cast_sampler = device.create_sampler(&SamplerDescriptor::default());

        let camera_bind_group_layout = Self::create_camera_bind_group_layout(&device);

        let position = Box::new(Vector3::new(0.0f32, 0.0f32, 0.0f32));
        let pitch = Box::new(0.0f32);
        let yaw = Box::new(0.0f32);
        let roll = Box::new(0.0f32);
        let fov = 77.0f32;
        let camera = Camera::new(
            &device,
            &camera_bind_group_layout,
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
            cast_render_pipeline_surface: None,
            cast_sampler,

            camera_bind_group_layout,

            surface: None,
            surface_config: None,

            submission_indices: Default::default(),
            submission_id: usize::MAX,

            mesh_collection: Default::default(),

            independent_renders: Default::default(),
            mesh_instanced_renders: Default::default(),
        };

        let result = Renderer(Arc::new(Mutex::new(inner)));

        Ok(result)
    }

    fn create_camera_bind_group_layout(device: &Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                // BindGroupLayoutEntry {
                //     binding: 0,
                //     count: None,
                //     ty: BindingType::Buffer {
                //         ty: wgpu::BufferBindingType::Uniform,
                //         has_dynamic_offset: false,
                //         min_binding_size: None
                //     },
                //     visibility: ShaderStages::VERTEX,
                // },
                BindGroupLayoutEntry {
                    binding: 1,
                    count: None,
                    ty: BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: std::num::NonZeroU64::new(
                            size_of::<CameraBufferStruct>() as u64,
                        ),
                    },
                    visibility: ShaderStages::VERTEX,
                },
            ],
        })
    }

    fn create_cast_render_pipeline(
        device: &Device,
        cast_bind_group_layout: &BindGroupLayout,
        format: TextureFormat,
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
                    format,
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
            &this.camera_bind_group_layout,
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
        transform: &Transform,
    ) -> Result<Model, error::ChoraError> {
        let device = self.device();
        let queue = self.queue();

        Ok(Model::new(
            &device,
            &queue,
            meshes,
            mutable,
            transform,
            MAX_FRAMES_IN_FLIGHT,
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

    pub fn create_spritesheet(&self) -> Spritesheet {
        let lock = self.0.lock().unwrap();

        Spritesheet::new_lock(self.clone(), &lock)
    }

    pub fn load_texture_from_path(&self, path: &Path) -> io::Result<Texture> {
        Texture::load_from_file_locked(self.clone(), &self.0.lock().unwrap(), path)
    }

    pub fn create_sampler(&self, address_mode: AddressMode, filter_mode: FilterMode) -> Sampler {
        let lock = self.0.lock().unwrap();
        Sampler::new_locked(self.clone(), &lock, address_mode, filter_mode)
    }

    pub fn create_render_pipeline(
        &self,
        code: &str,
        textures: &Vec<Texture>,
        sampler: Option<Sampler>,
        flags: RenderPipelineFlags,
    ) -> RenderPipeline {
        let lock = self.0.lock().unwrap();
        RenderPipeline::new(
            &lock.device,
            &lock.camera,
            &lock.camera_bind_group_layout,
            code,
            textures,
            sampler,
            vec![],
            flags,
        )
    }

    pub fn create_render_pipeline_with_remapping(
        &self,
        code: &str,
        textures: &Vec<Texture>,
        sampler: Option<Sampler>,
        remappings: Vec<crate::coordination::TextureRemapping>,
        flags: RenderPipelineFlags,
    ) -> RenderPipeline {
        let lock = self.0.lock().unwrap();
        RenderPipeline::new(
            &lock.device,
            &lock.camera,
            &lock.camera_bind_group_layout,
            code,
            textures,
            sampler,
            remappings,
            flags,
        )
    }

    pub fn add_to_render_queue(&mut self, model: &Model) -> Result<(), error::ChoraError> {
        for mesh in model.into_iter() {
            mesh.added.store(true, Ordering::Relaxed);
            self.add_mesh_to_render_queue(&mesh)?;
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
                .or_insert(InstancedRender::new(
                    clone,
                    &this,
                    mesh,
                    MAX_FRAMES_IN_FLIGHT,
                ));

            if mesh_collection_len == 2 {
                // Transitioning from independent → instanced.
                // mesh_collection is [new_mesh, old_mesh] after push_front.
                // Add old_mesh first so it occupies instance slot 0.
                let old_weak = this
                    .mesh_collection
                    .get(&mesh_address)
                    .and_then(|mc| mc.iter().nth(1))
                    .cloned();
                if let Some(old_weak) = old_weak {
                    if let Some(old_mesh) = old_weak.upgrade() {
                        instanced_render.add_mesh(&this, &old_mesh);
                    }
                }
                instanced_render.add_mesh(&this, &mesh);
                this.independent_renders.remove(&mesh_address).unwrap();
            } else {
                instanced_render.add_mesh(&this, &mesh);
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
        let should_remove = if let Some(ir) = this.mesh_instanced_renders.get(&mesh_address) {
            ir.remove_mesh(mesh);
            ir.mesh_count() == 0
        } else {
            false
        };
        if should_remove {
            this.mesh_instanced_renders.remove(&mesh_address);
            create_independent_render = true;
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

    /// Create a surface from a window handle for presenting rendered frames.
    ///
    /// This enables the renderer to display output to a window instead of just
    /// rendering to off-screen textures (headless mode).
    ///
    /// # Arguments
    /// * `window` - A type implementing HasWindowHandle and HasDisplayHandle (e.g., winit::Window)
    ///
    /// # Example
    /// ```no_run
    /// # use raw_window_handle::{HasWindowHandle, HasDisplayHandle};
    /// # let renderer = todo!();
    /// # let window: &dyn HasWindowHandle = todo!();
    /// # let display: &dyn HasDisplayHandle = todo!();
    /// renderer.create_surface(window, display).unwrap();
    /// ```
    pub fn create_surface(
        &self,
        window: &(impl raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle),
    ) -> Result<(), error::ChoraError> {
        let surface = unsafe {
            INSTANCE.create_surface_unsafe(
                wgpu::SurfaceTargetUnsafe::from_window(window)
                    .map_err(|_| error::ChoraError::FailedToCreateSurface)?,
            )
        }
        .map_err(|_| error::ChoraError::FailedToCreateSurface)?;

        let mut this = self.0.lock().unwrap();
        let adapter = &this._adapter;

        // Get surface capabilities
        let surface_caps = surface.get_capabilities(adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let camera = &this.camera;
        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: camera.width(),
            height: camera.height(),
            present_mode: PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&this.device, &config);

        // Build a surface-specific blit pipeline that matches the swapchain format.
        // The offscreen cast_render_pipeline (Rgba8Unorm) is left untouched so that
        // the atlas/spritesheet copy path always works regardless of swapchain format.
        this.cast_render_pipeline_surface = Some(Self::create_cast_render_pipeline(
            &this.device,
            &this.cast_bind_group_layout,
            surface_format,
        ));

        this.surface = Some(surface);
        this.surface_config = Some(config);

        Ok(())
    }

    /// Present the current frame to the window surface.
    ///
    /// This copies the main camera's output texture to the window's swapchain.
    /// Requires that create_surface() has been called first.
    ///
    /// # Returns
    /// Ok(()) if the frame was presented successfully
    /// Err if no surface has been configured or presentation fails
    ///
    /// # Example
    /// ```no_run
    /// # let renderer = todo!();
    /// // After rendering...
    /// renderer.present().unwrap();
    /// ```
    pub fn present(&self) -> Result<(), error::ChoraError> {
        let this = self.0.lock().unwrap();

        let surface = this
            .surface
            .as_ref()
            .ok_or(error::ChoraError::NoSurfaceConfigured)?;

        let output = surface
            .get_current_texture()
            .map_err(|_| error::ChoraError::FailedToAcquireSwapchainTexture)?;

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Get camera's output texture and create a bind group for it
        let camera = &this.camera;
        let camera_texture_view = camera.current_output_texture_view();

        // Create a temporary bind group for the camera texture
        let bind_group = this.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Present Bind Group"),
            layout: &this.cast_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&camera_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&this.cast_sampler),
                },
            ],
        });

        let mut encoder = this
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Present Encoder"),
            });

        // Use the cast render pipeline to blit the camera texture to the surface
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Present Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            let surface_pipeline = this
                .cast_render_pipeline_surface
                .as_ref()
                .unwrap_or(&this.cast_render_pipeline);
            render_pass.set_pipeline(surface_pipeline);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        this.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }

    /// Resize the swapchain surface.
    ///
    /// Call this when the window is resized to update the surface configuration.
    ///
    /// # Arguments
    /// * `new_width` - The new width in pixels
    /// * `new_height` - The new height in pixels
    ///
    /// # Example
    /// ```no_run
    /// # let renderer = todo!();
    /// renderer.resize_surface(1920, 1080).unwrap();
    /// ```
    pub fn resize_surface(&self, new_width: u32, new_height: u32) -> Result<(), error::ChoraError> {
        let mut this = self.0.lock().unwrap();

        if this.surface.is_none() || this.surface_config.is_none() {
            return Err(error::ChoraError::NoSurfaceConfigured);
        }

        // Update the config
        let config = this.surface_config.as_mut().unwrap();
        config.width = new_width;
        config.height = new_height;

        // Clone config and device, then configure surface
        let config_clone = config.clone();
        let device = this.device.clone();
        let surface = this.surface.as_ref().unwrap();

        surface.configure(&device, &config_clone);

        Ok(())
    }

    /// Drop the wgpu Surface while the window is still alive.
    ///
    /// Call this in a `LoopExiting` (or equivalent) handler before the window
    /// is freed. The Surface holds a raw window handle internally; if it is
    /// dropped after the window, the destructor fires on a dangling pointer and
    /// causes a segfault. Calling this method detaches the surface early so the
    /// rest of `RendererInner` can outlive the window safely.
    pub fn drop_surface(&self) {
        let mut this = self.0.lock().unwrap();
        this.surface = None;
        this.surface_config = None;
        this.cast_render_pipeline_surface = None;
    }

    pub fn render(&self) -> Result<(), error::ChoraError> {
        let mut this = self.0.lock().unwrap();
        // Switch up the submission id (Keep in mind we start at usize::MAX and add one to get to zero)
        this.submission_id = this.submission_id.wrapping_add(1) % MAX_FRAMES_IN_FLIGHT;

        // Wait on a possible submission
        let submission_id = this.submission_id;
        let possible_submission = replace(&mut this.submission_indices[submission_id], None);
        if let Some(submission) = possible_submission {
            this.device
                .poll(wgpu::wgt::PollType::WaitForSubmissionIndex(submission))
                .map_err(|e| ChoraError::SubmissionPollingError { err: e })?;
        }

        // 1. Acquire the camera's current output texture view
        let camera = &this.camera;
        let camera_view = camera.current_output_texture_view();
        let camera_bindgroup = camera.bind_group(0).unwrap();
        let depth_view = camera.depth_texture_view();

        let mut encoder = this
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Pre-pass: collect per-instance model matrices and upload to GPU instance buffers.
        // Clone device/queue so we avoid simultaneous field borrows through MutexGuard.
        let device = this.device.clone();
        let queue = this.queue.clone();
        let instanced_addrs: Vec<*const c_void> =
            this.mesh_instanced_renders.keys().cloned().collect();
        // Render pass is in its own scope so it drops before encoder.finish().
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &camera_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            for (mesh_address, mesh_list) in &this.mesh_collection {
                if let Some(instanced_render) = this.mesh_instanced_renders.get(mesh_address) {
                    if let Some(pipeline) = instanced_render.active_pipeline() {
                        let pipeline_lock = pipeline.inner.lock().unwrap();
                        let allow_object_uniform = pipeline
                            .flags
                            .contains(RenderPipelineFlags::ALLOW_OBJECT_UNIFORM);

                        render_pass.set_pipeline(&pipeline_lock._render_pipeline);
                        render_pass.set_bind_group(0, &camera_bindgroup, &[]);
                        render_pass.set_bind_group(
                            pipeline_lock.texture_bind_group_index,
                            &pipeline_lock._texture_bind_group,
                            &[],
                        );

                        // Flush per-instance model matrices before drawing.
                        for weak_mesh in mesh_list.iter() {
                            if let Some(model) = weak_mesh.model.as_ref().and_then(|wm| {
                                let inner = wm.0.upgrade()?;
                                Some(unsafe {
                                    mem::transmute::<Arc<crate::model::ModelInner>, Model>(inner)
                                })
                            }) {
                                model.update_matrix_and_upload(
                                    &this.device,
                                    &this.queue,
                                    submission_id,
                                );
                            }
                        }

                        // All meshes in the group share the same vertex/index data.
                        if let Some(first_weak) = mesh_list.iter().next() {
                            if let Some(mesh_arc) = first_weak.upgrade() {
                                let mesh_lock = mesh_arc.inner.lock().unwrap();
                                render_pass
                                    .set_vertex_buffer(0, mesh_lock._vertex_buffer.slice(..));

                                if allow_object_uniform {
                                    if let Some(ibuf) =
                                        instanced_render.instance_buffer(submission_id)
                                    {
                                        render_pass.set_vertex_buffer(1, ibuf.slice(..));
                                    }
                                }
                                render_pass.set_index_buffer(
                                    mesh_lock._index_buffer.slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );

                                let index_count = mesh_lock._index_buffer.size() as u32 / 4;
                                let instance_count = instanced_render.mesh_count() as u32;
                                render_pass.draw_indexed(0..index_count, 0, 0..instance_count);
                            }
                        }
                    }
                } else if let Some(weak_pipeline) = this.independent_renders.get(mesh_address) {
                    if let Some(pipeline) = weak_pipeline.upgrade() {
                        let pipeline_lock = pipeline.inner.lock().unwrap();
                        render_pass.set_pipeline(&pipeline_lock._render_pipeline);
                        render_pass.set_bind_group(0, &camera_bindgroup, &[]);
                        render_pass.set_bind_group(
                            pipeline_lock.texture_bind_group_index,
                            &pipeline_lock._texture_bind_group,
                            &[],
                        );

                        for weak_mesh in mesh_list.iter() {
                            if let Some(mesh_inner) = weak_mesh.upgrade() {
                                let mesh_lock = mesh_inner.inner.lock().unwrap();
                                render_pass
                                    .set_vertex_buffer(0, mesh_lock._vertex_buffer.slice(..));
                                if let Some(model) = weak_mesh.model.as_ref().and_then(|wm| {
                                    let inner = wm.0.upgrade()?;
                                    Some(unsafe {
                                        mem::transmute::<Arc<crate::model::ModelInner>, Model>(
                                            inner,
                                        )
                                    })
                                }) {
                                    model.update_matrix_and_upload(
                                        &this.device,
                                        &this.queue,
                                        this.submission_id,
                                    );
                                    render_pass.set_vertex_buffer(
                                        1,
                                        model.model_buffer(this.submission_id).slice(..),
                                    );
                                }
                                render_pass.set_index_buffer(
                                    mesh_lock._index_buffer.slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );
                                let index_count = mesh_lock._index_buffer.size() as u32 / 4;
                                render_pass.draw_indexed(0..index_count, 0, 0..1);
                            }
                        }
                    }
                }
            }
        } // render_pass drops here, releasing the borrow on encoder

        this.submission_indices[submission_id] =
            Some(this.queue.submit(std::iter::once(encoder.finish())));

        Ok(())
    }

    pub fn downgrade(&self) -> WeakRenderer {
        WeakRenderer(Arc::downgrade(&self.0))
    }
}

impl WeakRenderer {
    pub fn upgrade(&self) -> Option<Renderer> {
        Weak::upgrade(&self.0).map(|r| Renderer(r))
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

        // bs_shader.wgsl defines its own VertexInput struct, so use OVERRIDE_VERTEX_INPUT
        // to prevent the rewriter from injecting the incompatible RawVertexInput format.
        let render_pipeline = renderer.create_render_pipeline(
            shader,
            &textures,
            Some(sampler),
            RenderPipelineFlags::OVERRIDE_VERTEX_INPUT,
        );

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
            parent_model: Mutex::new(None),
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

    /// Test to demonstrate generated shader code with texture remapping
    #[test]
    pub fn test_generated_shader_output() {
        use crate::coordination::{Point2D, Rectangle, TextureRemapping};
        use std::path::Path;

        let renderer = Renderer::new(512, 512, 2).unwrap();

        // Original shader source with just one atlas texture
        // The textureSample10, textureSample20, textureSample30 functions will be auto-generated
        let original_shader = r#"
@group(1) @binding(0) var atlas_texture: texture_2d<f32>;
@group(1) @binding(1) var my_sampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    let uv = vec2<f32>(0.5, 0.5);

    // These functions don't exist yet - they'll be generated by the remapping!
    // They represent virtual textures 10, 20, and 30 that all map to regions in atlas_texture
    let color_a = textureSample10(atlas_texture, my_sampler, uv);
    let color_b = textureSample20(atlas_texture, my_sampler, uv);
    let color_c = textureSample30(atlas_texture, my_sampler, uv);

    return mix(mix(color_a, color_b, 0.5), color_c, 0.33);
}
"#;

        println!("\n========== ORIGINAL SHADER ==========");
        println!("{}", original_shader);

        // Create a texture for the pipeline
        let texture = renderer
            .load_texture_from_path(Path::new("kenya.jpg"))
            .unwrap();

        let sampler = renderer.create_sampler(AddressMode::Repeat, FilterMode::Linear);

        // Create texture remappings simulating sprite atlas regions
        // These map "virtual" texture indices (10, 20, 30) to the actual atlas texture at binding 0
        let remappings = vec![
            TextureRemapping::new(
                10,                                                             // Virtual texture 10
                0, // Maps to atlas at binding 0
                Rectangle::new(Point2D::new(0.0, 0.0), Point2D::new(0.5, 0.5)), // Top-left quadrant
            ),
            TextureRemapping::new(
                20,                                                             // Virtual texture 20
                0, // Maps to same atlas at binding 0
                Rectangle::new(Point2D::new(0.5, 0.0), Point2D::new(1.0, 0.5)), // Top-right quadrant
            ),
            TextureRemapping::new(
                30,                                                             // Virtual texture 30
                0, // Maps to same atlas at binding 0
                Rectangle::new(Point2D::new(0.0, 0.5), Point2D::new(0.5, 1.0)), // Bottom-left quadrant
            ),
        ];

        // Create render pipeline with remapping
        let pipeline = renderer.create_render_pipeline_with_remapping(
            original_shader,
            &vec![texture],
            Some(sampler),
            remappings,
            RenderPipelineFlags::empty(),
        );

        // Get the generated shader code
        let generated_shader = pipeline.shader_code();

        println!("\n========== GENERATED SHADER ==========");
        println!("{}", generated_shader);
        println!("======================================\n");

        // Verify that textureSample functions were generated
        assert!(generated_shader.contains("fn textureSample10("));
        assert!(generated_shader.contains("fn textureSample20("));
        assert!(generated_shader.contains("fn textureSample30("));

        // Verify UV remapping coordinates are in the functions
        assert!(generated_shader.contains("vec2<f32>(0, 0)"));
        assert!(generated_shader.contains("vec2<f32>(0.5, 0.5)"));
        assert!(generated_shader.contains("vec2<f32>(1, 0.5)"));
        assert!(generated_shader.contains("vec2<f32>(0.5, 1)"));
    }
}
