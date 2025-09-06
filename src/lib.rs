use std::collections::HashMap;
use std::ffi::c_void;
use std::io;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};
use cgmath::Vector3;

use wgpu::Adapter;
use wgpu::Device;
use wgpu::Queue;
use wgpu::BackendOptions;
use wgpu::Backends;
use wgpu::Instance;
use wgpu::InstanceDescriptor;
use wgpu::InstanceFlags;
use wgpu::MemoryBudgetThresholds;
use wgpu::RequestAdapterOptions;
use wgpu::wgt::DeviceDescriptor;
use wgpu::wgt::TextureFormat;

use crate::camera::Camera;
use crate::linked_list::LinkedList;
use crate::mesh::{Mesh, WeakMesh};
use crate::model::Model;
use crate::render_pipeline::RenderPipeline;
use crate::texture::Texture;

pub mod error;
pub mod camera;
pub mod mesh;
pub mod model;
mod linked_list;
pub mod texture;
mod render_pipeline;

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

    // Mesh Database (I think I don't really know what this is called)
    mesh_collection: HashMap<*const c_void, LinkedList<WeakMesh>>,

    independent_renders: HashMap<*const c_void, ()>,
    instanced_renders: HashMap<*const c_void, usize>,
}

#[derive(Clone)]
pub struct Renderer(Arc<Mutex<RendererInner>>);

impl Renderer {
    pub fn new(buffers: usize) -> Result<Self, error::ChoraError> {
        let adapter = pollster::block_on(INSTANCE.request_adapter(&RequestAdapterOptions {
            ..Default::default()
        })).map_err(|_| error::ChoraError::FailedToFindAdapter {})?;

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: Some("0x99 CRAYON CHORA"),
            ..Default::default()
        })).map_err(|_| error::ChoraError::FailedGettingSuitableDevice {})?;

        let inner = RendererInner {
            adapter,
            device,
            queue,
            buffers,

            mesh_collection: Default::default(),
            independent_renders: Default::default(),
            instanced_renders: Default::default(),
        };

        Ok(Renderer(Arc::new(Mutex::new(inner))))
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
        Ok(Mesh::new(self.clone(), vertices, indices, render_pipeline))
    }

    pub fn create_model(&self, meshes: Vec<Mesh>, mutable: bool,
                        position: &Vector3<f32>, rotation: &Vector3<f32>, scale: &Vector3<f32>)
                        -> Result<Model, error::ChoraError> {
        let this = self.0.lock().unwrap();

        Ok(Model::new(
            &this.device,
            meshes,
            mutable,
            position as *const _ as _,
            rotation as *const _ as _,
            scale as *const  _ as _
        ))
    }

    pub fn create_texture(&self, width: u32, height: u32, format: TextureFormat, data: Option<&[u8]>) -> Texture {
        Texture::new(self.clone(), width, height, format, data)
    }

    pub fn load_texture_from_path(&self, path: &Path) -> io::Result<Texture> {
        Texture::load_from_file(self.clone(), path)
    }

    pub fn create_render_pipeline(&self, code: &str, textures: &LinkedList<Texture>) -> RenderPipeline {
        let lock = self.0.lock().unwrap();
        RenderPipeline::new(&lock.device, code, textures)
    }

    pub fn add_mesh_to_render_queue(&self, mesh: &Mesh) -> Result<(), error::ChoraError> {
        let mesh_address = mesh.inner.as_ref() as *const _ as *const c_void;
        let weak_mesh = mesh.downgrade();
        let mut this = self.0.lock().unwrap();

        // Organize the mesh into groups
        let mesh_collection= this.mesh_collection
            .entry(mesh_address)
            .or_insert(LinkedList::new());

        let _mesh_collection_node = mesh_collection
            .push_front(weak_mesh.clone());

        // Check for instanceable meshes
        let mesh_collection = this.mesh_collection
            .get(&(mesh.inner.as_ref() as *const _ as _)).unwrap();
        let mesh_collection_len = mesh_collection.len();

        if mesh_collection_len > 1 {
            if mesh_collection_len == 2 {
                this.independent_renders.remove(&mesh_address);
                this.instanced_renders.insert(mesh_address, 1);
            }
            *this.instanced_renders.get_mut(&mesh_address).unwrap() += 1;
        } else {
            // Create a single independent render.
            this.independent_renders.insert(mesh_address, ());
        }

        Ok(())
    }

    pub fn add_to_render_queue(&self, model: Model) -> Result<(), error::ChoraError> {
        for mesh in model.into_iter() {
            self.add_mesh_to_render_queue(&mesh)?;
        }
        Ok(())
    }

    pub(crate) fn remove_mesh_from_render_queue(&self, mesh: &Mesh) {
        let mesh_address = mesh.inner.as_ref() as *const _ as *const c_void;
        let mut this = self.0.lock().unwrap();

        this.independent_renders.remove(&mesh_address);
        if let Some(count) = this.instanced_renders.get_mut(&mesh_address) {
            *count -= 1;
            if *count == 1 {
                // Create a new independent render.
                this.independent_renders.insert(mesh_address, ());
                this.instanced_renders.remove(&mesh_address);
            }
        }
    }

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
        let renderer = Renderer::new(2).unwrap();

        let pos = cgmath::vec3(0.0f32, 0.0f32, 0.0f32);
        let pitch = 0.0f32;
        let yaw = 0.0f32;
        let roll = 0.0f32;
        renderer.create_camera(512, 512, true, 77.0, false, pos.as_ref(), &pitch, &yaw, &roll).unwrap();
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
        let renderer = Renderer::new(2).unwrap();

        // Camera setup
        let pos = cgmath::vec3(0.0f32, 0.0f32, 0.0f32);
        let pitch = 0.0f32;
        let yaw = 0.0f32;
        let roll = 0.0f32;
        let camera = renderer.create_camera(
            512,
            512,
            true,
            77.0,
            false,
            pos.as_ref(),
            &pitch,
            &yaw,
            &roll,
        ).unwrap();

        let shader = include_str!("../src/bs_shader.wgsl");

        let texture = renderer.load_texture_from_path(Path::new("kenya.jpg")).unwrap();
        let textures = LinkedList::from([texture]);
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
            assert_eq!(lock.instanced_renders.len(), 0, "No instanced renders should exist");
            assert_eq!(lock.mesh_collection.len(), 1, "Should have one mesh collection");
            assert_eq!(lock.mesh_collection.values().next().unwrap().len(), 1, "Collection should have one mesh");
        }

        // Test identical mesh (should become instanced)
        renderer.add_mesh_to_render_queue(&i_triangle1).unwrap();
        {
            let lock = renderer.0.lock().unwrap();
            assert_eq!(lock.independent_renders.len(), 0, "No independent renders should remain");
            assert_eq!(lock.instanced_renders.len(), 1, "Should have one instanced render");
            assert_eq!(lock.mesh_collection.len(), 1, "Should have one mesh collection");
            assert_eq!(lock.mesh_collection.values().next().unwrap().len(), 2, "Collection should have two meshes");

            let i_render = lock.instanced_renders.iter().nth(0).unwrap();
            assert_eq!(*i_render.1, 2, "Instance count should be 2");
        }

        // Test different mesh (should be independent)
        renderer.add_mesh_to_render_queue(&s_triangle2).unwrap();
        {
            let lock = renderer.0.lock().unwrap();
            assert_eq!(lock.independent_renders.len(), 1, "Should have one independent render");
            assert_eq!(lock.instanced_renders.len(), 1, "Should have one instanced render");
            assert_eq!(lock.mesh_collection.len(), 2, "Should have two mesh collections");

            let i_render = lock.instanced_renders.iter().nth(0).unwrap();
            assert_eq!(*i_render.1, 2, "Instance count should remain 2");
        }

        // Clean up
        drop(i_triangle0);
        drop(i_triangle1);
        drop(s_triangle2);
        drop(camera);
    }
}
