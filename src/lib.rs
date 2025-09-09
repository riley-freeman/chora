use cgmath::Vector3;
use etagere::{size2, Allocation, AtlasAllocator};
use std::collections::HashMap;
use std::ffi::c_void;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use wgpu::wgt::DeviceDescriptor;
use wgpu::wgt::TextureFormat;
use wgpu::Adapter;
use wgpu::BackendOptions;
use wgpu::Backends;
use wgpu::Device;
use wgpu::Instance;
use wgpu::InstanceDescriptor;
use wgpu::InstanceFlags;
use wgpu::MemoryBudgetThresholds;
use wgpu::Queue;
use wgpu::RequestAdapterOptions;

use crate::camera::Camera;
use crate::linked_list::LinkedList;
use crate::mesh::{Mesh, WeakMesh};
use crate::model::Model;
use crate::render_pipeline::{RenderPipeline, WeakRenderPipeline};
use crate::render_target::RenderTarget;
use crate::texture::Texture;

pub mod error;
pub mod camera;
pub mod mesh;
pub mod model;
mod linked_list;
pub mod texture;
mod render_pipeline;
mod render_target;

const MAX_TEXTURE_SIZE: u32 = 4096;
const MAX_BINDABLE_TEXTURE_COUNT: usize = 16;

static INSTANCE :LazyLock<Instance> = LazyLock::new(|| {
    Instance::new(&InstanceDescriptor {
        backends: Backends::PRIMARY,
        #[cfg(debug_assertions)]
        flags: InstanceFlags::DEBUG,
        #[cfg(not(debug_assertions))]
        flags: InstanceFlags::empty(),
        memory_budget_thresholds: MemoryBudgetThresholds {
            for_resource_creation: None,
            for_device_loss: None
        },
        backend_options: BackendOptions {..Default::default()}
    })
});

struct RendererInner {
    adapter: Adapter,
    device: Device,
    queue: Queue,
    buffers: usize,

    // Final / Main output
    camera: Camera,
    position: Box<Vector3<f32>>,
    pitch: Box<f32>,
    yaw: Box<f32>,
    roll: Box<f32>,

    // Mesh Database (I think I don't really know what this is called)
    mesh_collection: HashMap<*const c_void, LinkedList<WeakMesh>>,
    mesh_texture_collection: HashMap<
        *const c_void,
        HashMap<*const c_void, HousedTexture>
    >,
    mesh_atlas_collection: HashMap<*const c_void, LinkedList<TextureAtlas>>,
    mesh_housed_texture_collection: HashMap<*const c_void, (HousedTexture, usize)>,
    mesh_render_pipeline_collection: HashMap<*const c_void, WeakRenderPipeline>,
    mesh_instanced_renders: HashMap<*const c_void, HashMap<*const c_void, InstancedRender>>,

    independent_renders: HashMap<*const c_void, WeakRenderPipeline>,
}

impl RendererInner {
}

#[derive(Clone)]
pub struct Renderer(Arc<Mutex<RendererInner>>);

impl Renderer {
    pub fn new(width: u32, height: u32, buffers: usize) -> Result<Self, error::ChoraError> {
        let adapter = pollster::block_on(INSTANCE.request_adapter(&RequestAdapterOptions {
            ..Default::default()
        })).map_err(|_| error::ChoraError::FailedToFindAdapter {})?;

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: Some("0x99 CRAYON CHORA"),
            ..Default::default()
        })).map_err(|_| error::ChoraError::FailedGettingSuitableDevice {})?;

        let position = Box::new(Vector3::new(0.0f32, 0.0f32, 0.0f32));
        let pitch = Box::new(0.0f32);
        let yaw = Box::new(0.0f32);
        let roll = Box::new(0.0f32);
        let fov = 77.0f32;

        let camera = Camera::new(
            &device,
            width, height,
            buffers,
            false, fov, false,
            position.as_ref().as_ref(),
            &pitch, &yaw, &roll,
        )?;

        let inner = RendererInner {
            adapter,
            device,
            queue,
            buffers,

            camera,
            position,
            pitch,
            yaw,
            roll,

            mesh_collection: Default::default(),
            mesh_texture_collection: Default::default(),
            mesh_render_pipeline_collection: Default::default(),
            mesh_housed_texture_collection: Default::default(),
            mesh_atlas_collection: Default::default(),

            independent_renders: Default::default(),
            mesh_instanced_renders: Default::default(),
        };

        let result = Renderer(Arc::new(Mutex::new(inner)));
        Ok(result)
    }

    pub fn main_camera(&self) -> Camera {
        let this = self.0.lock().unwrap();
        this.camera.clone()
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
            width, height,
            this.buffers,
            hdr, fov, orthographic,
            position,
            pitch, yaw, roll,
        )
    }

    pub fn create_mesh(&self, vertices: &[f32], indices: &[i32], render_pipeline: RenderPipeline) -> Result<Mesh, error::ChoraError> {
        let device = self.0.lock().unwrap().device.clone();
        Ok(Mesh::new(self.clone(), &device, vertices, indices, render_pipeline))
    }

    pub fn create_model(&self, meshes: Vec<Mesh>, mutable: bool,
                        position: &Vector3<f32>, rotation: &Vector3<f32>, scale: &Vector3<f32>)
                        -> Result<Model, error::ChoraError> {
        let device = self.0.lock().unwrap().device.clone();

        Ok(Model::new(
            &device,
            meshes,
            mutable,
            position as *const _ as _,
            rotation as *const _ as _,
            scale as *const  _ as _
        ))
    }

    pub fn create_texture(&self, width: u32, height: u32, format: TextureFormat, data: Option<&[u8]>) -> Texture {
        let lock = self.0.lock().unwrap();
        let device = lock.device.clone();
        let queue = lock.queue.clone();
        drop(lock);

        Texture::new(self.clone(), &device, &queue, width, height, format, data)
    }

    pub fn load_texture_from_path(&self, path: &Path) -> io::Result<Texture> {
        let lock = self.0.lock().unwrap();
        let device = lock.device.clone();
        let queue = lock.queue.clone();
        drop(lock);

        Texture::load_from_file(self.clone(), &device, &queue, path)
    }

    pub fn create_render_pipeline(&self, code: &str, textures: &Vec<Texture>) -> RenderPipeline {
        let lock = self.0.lock().unwrap();
        RenderPipeline::new(&lock.device, &lock.camera, code, textures)
    }

    pub fn add_mesh_to_render_queue(&mut self, mesh: &Mesh) -> Result<(), error::ChoraError> {
        let mesh_address = mesh.inner.as_ref() as *const _ as *const c_void;
        let weak_mesh = mesh.downgrade();

        let render_pipeline = mesh.render_pipeline();

        let mut this = self.0.lock().unwrap();

        // Organize the mesh into groups
        let mesh_collection= this.mesh_collection
            .entry(mesh_address)
            .or_insert(LinkedList::new());

        let _mesh_collection_node = mesh_collection.push_front(weak_mesh.clone());

        // Check for instanceable meshes
        let mesh_collection = this.mesh_collection
            .get(&(mesh.inner.as_ref() as *const _ as _)).unwrap();
        let mesh_collection_len = mesh_collection.len();

        if mesh_collection_len > 1 {
            if mesh_collection_len == 2 {
                let weak_render_pipeline = this.independent_renders
                    .remove(&mesh_address)
                    .unwrap();

                if let Some(og_render_pipeline) = weak_render_pipeline.upgrade() {
                    self.handle_new_instance_render_pipeline(&mut this, mesh_address, og_render_pipeline);
                }
            }


            self.handle_new_instance_render_pipeline(&mut this, mesh_address, render_pipeline);

            // Build new render groups.
        } else {
            // Create a single independent render.
            this.independent_renders.insert(mesh_address, render_pipeline.downgrade());
        }

        Ok(())
    }

    pub fn add_to_render_queue(&mut self, model: Model) -> Result<(), error::ChoraError> {
        for mesh in model.into_iter() {
            self.add_mesh_to_render_queue(&mesh)?;
        }
        Ok(())
    }

    pub(crate) fn remove_mesh_from_render_queue(&self, mesh: &Mesh) {
        let render_pipeline = mesh.render_pipeline();

        let mesh_address = mesh.inner.as_ref() as *const _ as *const c_void;
        let pipeline_address = render_pipeline.inner.as_ref() as *const _ as *const c_void;

        let mut this = self.0.lock().unwrap();

        this.independent_renders.remove(&mesh_address);
        let mut create_independent_render = false;
        if let Some(instanced_renders) = this.mesh_instanced_renders.get_mut(&mesh_address) {
            if let Some(render) = instanced_renders.get_mut(&pipeline_address) {
                let prev_count = render.count.fetch_sub(1, Ordering::Relaxed);
                if prev_count == 2 { instanced_renders.remove(&pipeline_address); }
                create_independent_render = true;
            }
        }

        if create_independent_render {
            this.independent_renders.insert(mesh_address, render_pipeline.downgrade());
        }
    }


    fn handle_new_instance_render_pipeline(
        &self,
        this: &mut MutexGuard<RendererInner>,
        mesh_address: *const c_void,
        pipeline: RenderPipeline
    ) {
        let mut cloned = self.clone();
        let pipeline_address = pipeline.inner.as_ref() as *const _ as *const c_void;

        // Create a new render pipeline
        let shader_code = pipeline.shader_code(); // TODO: make this shader dynamic.

        let instanced_renders = this.mesh_instanced_renders.get_mut(&mesh_address);

        match instanced_renders {
            Some(instanced_renders) => {
                let render = instanced_renders
                    .get_mut(&pipeline_address)
                    .unwrap();

                render.count.fetch_add(1, Ordering::Relaxed);
            }
            None => {
                let device = this.device.clone();
                let camera = this.camera.clone();
                let textures = pipeline.textures();
                let atlas_collection = this.mesh_atlas_collection
                    .entry(mesh_address)
                    .or_insert(LinkedList::new());

                let housed_textures: Vec<HousedTexture> = textures.iter().map(|texture| {
                    cloned.handle_new_instance_texture(atlas_collection, &device, texture.clone())
                }).collect();
                let textures: Vec<Texture> = housed_textures
                    .iter()
                    .map(|housed_textures| housed_textures.atlas.clone()).collect();

                let instanced_renders = this.mesh_instanced_renders
                    .entry(mesh_address)
                    .or_insert(HashMap::new());


                for (_, render) in instanced_renders.iter_mut() {
                    let render = render.clone();
                    let mut housed_textures_lock = render.housed_textures.lock().unwrap();
                    if housed_textures_lock.len() >= MAX_BINDABLE_TEXTURE_COUNT { continue; }

                    // Merge the new textures into the render pipeline.
                    let mut render_textures: Vec<Texture> = housed_textures_lock.iter().map(
                        |texture| texture.atlas.clone()
                    ).collect();
                    render_textures.extend_from_slice(textures.as_slice());

                    let new_render_pipeline = RenderPipeline::new(&device, &camera, &shader_code, &render_textures);

                    // Update the info in the renderer
                    render.count.fetch_add(1, Ordering::Relaxed);
                    *render.pipeline.lock().unwrap() = new_render_pipeline;
                    housed_textures_lock.extend(housed_textures);

                    instanced_renders.insert(pipeline_address, render.clone());
                    return;
                }

                let render_pipeline = RenderPipeline::new(&device, &camera, &shader_code, &textures);
                let instanced_render = InstancedRender {
                    count: Arc::new(AtomicUsize::new(1)),
                    pipeline: Arc::new(Mutex::new(render_pipeline)),
                    housed_textures: Arc::new(Mutex::new(housed_textures)),
                };
                instanced_renders.insert(pipeline_address, instanced_render);
            }
        }
    }

    fn handle_new_instance_texture(
        &mut self,
        atlas_collection: &mut LinkedList<TextureAtlas>,
        device: &Device,
        texture: Texture
    ) -> HousedTexture {
        let width = texture.width();
        let height = texture.height();
        for atlas in atlas_collection.into_iter() {
            let allocation = atlas.allocator.allocate(size2(
                width as _, height as _
            ));
            if allocation.is_some() {
                return HousedTexture {
                    atlas: atlas.texture.clone(),
                    _allocation: allocation.unwrap(),
                };
            }
        }

        let new_texture = Texture::empty(
            self.clone(),
            device,
            MAX_TEXTURE_SIZE,
            MAX_TEXTURE_SIZE,
            TextureFormat::Rgba8Unorm,
        );
        let mut allocator = AtlasAllocator::new(size2(
            MAX_TEXTURE_SIZE as _, MAX_TEXTURE_SIZE as _
        ));
        let allocation = allocator.allocate(size2(
            width as _, height as _
        )).unwrap();

        atlas_collection.push_front(TextureAtlas {
            texture: new_texture.clone(),
            allocator,
        });

        HousedTexture {
            atlas: new_texture,
            _allocation: allocation,
        }
    }
}

#[derive(Clone)]
struct InstancedRender {
    pipeline: Arc<Mutex<RenderPipeline>>,
    housed_textures: Arc<Mutex<Vec<HousedTexture>>>,
    count: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct HousedTexture {
    atlas: Texture,
    _allocation: Allocation
}

struct TextureAtlas {
    allocator: AtlasAllocator,
    texture: Texture,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::drop;

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

        let texture = renderer.load_texture_from_path(Path::new("kenya.jpg")).unwrap();
        let textures = Vec::from([texture]);
        let render_pipeline = renderer.create_render_pipeline(shader, &textures);

        // Create test meshes
        let vertices = [
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
            [-0.0, -0.5, 0.0],
        ];
        let vertices = vertices.iter().flat_map(|v| v.iter()).copied().collect::<Vec<f32>>();
        let indices = [0, 1, 2];

        let i_triangle0 = renderer.create_mesh(&vertices, &indices, render_pipeline.clone()).unwrap();
        let i_triangle1 = Mesh {
            inner: Arc::clone(&i_triangle0.inner),
        };

        let s_triangle2 = renderer.create_mesh(&vertices, &indices, render_pipeline.clone()).unwrap();

        // Test single mesh (should be independent)
        renderer.add_mesh_to_render_queue(&i_triangle0).unwrap();
        {
            let lock = renderer.0.lock().unwrap();
            assert_eq!(lock.independent_renders.len(), 1, "Single mesh should be independent");
            assert_eq!(lock.mesh_instanced_renders.len(), 0, "No instanced renders should exist");
            assert_eq!(lock.mesh_collection.len(), 1, "Should have one mesh collection");
            assert_eq!(lock.mesh_collection.values().next().unwrap().len(), 1, "Collection should have one mesh");
        }

        // Test identical mesh (should become instanced)
        renderer.add_mesh_to_render_queue(&i_triangle1).unwrap();
        {
            let lock = renderer.0.lock().unwrap();
            assert_eq!(lock.independent_renders.len(), 0, "No independent renders should remain");
            assert_eq!(lock.mesh_instanced_renders.len(), 1, "Should have one instanced render");
            assert_eq!(lock.mesh_collection.len(), 1, "Should have one mesh collection");
            assert_eq!(lock.mesh_collection.values().next().unwrap().len(), 2, "Collection should have two meshes");

            let i_render = lock.mesh_instanced_renders.iter().nth(0).unwrap();
            let count = i_render.1.iter().nth(0).unwrap().1.count.load(Ordering::Relaxed);
            assert_eq!(count, 2, "Instance count should be 2");
        }

        // Test different mesh (should be independent)
        renderer.add_mesh_to_render_queue(&s_triangle2).unwrap();
        {
            let lock = renderer.0.lock().unwrap();
            assert_eq!(lock.independent_renders.len(), 1, "Should have one independent render");
            assert_eq!(lock.mesh_instanced_renders.len(), 1, "Should have one instanced render");
            assert_eq!(lock.mesh_collection.len(), 2, "Should have two mesh collections");

            let i_render = lock.mesh_instanced_renders.iter().nth(0).unwrap();
            let count = i_render.1.iter().nth(0).unwrap().1.count.load(Ordering::Relaxed);
            assert_eq!(count, 2, "Instance count should remain 2");
        }

        // Clean up
        drop(i_triangle0);
        drop(i_triangle1);
        drop(s_triangle2);
    }
}
