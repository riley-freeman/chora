use std::slice;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Weak;
use wgpu::BufferUsages;
use wgpu::util::BufferInitDescriptor;
use wgpu::util::DeviceExt;
use crate::render_pipeline::RenderPipeline;
use crate::Renderer;

pub(crate) struct MeshInner {
    _vertex_buffer: wgpu::Buffer,
    _index_buffer: wgpu::Buffer,

    renderer: Renderer,
    pipeline: RenderPipeline,

    // TODO: Add some shader or pipeline or something to this...
}

impl Drop for MeshInner {
    fn drop(&mut self) {

    }
}

pub struct Mesh {
    pub(crate) inner: Arc<Mutex<MeshInner>>,
}
#[derive(Debug, Clone)]
pub struct WeakMesh(Weak<Mutex<MeshInner>>);

impl Drop for Mesh {
    fn drop(&mut self) {
        let lock = self.inner.lock().unwrap();
        lock.renderer.remove_mesh_from_render_queue(&self);
    }
}

impl Mesh {
    pub fn new(renderer: Renderer, vertices: &[f32], indices: &[i32], render_pipeline: RenderPipeline) -> Self {
        let lock = renderer.0.lock().unwrap();
        let device = &lock.device;

        let vertex_data: &[u8] = unsafe { slice::from_raw_parts(vertices.as_ptr() as *const u8, vertices.len() * size_of::<f32>()) };

        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            usage: BufferUsages::VERTEX,
            contents: vertex_data,
            label: None,
        });

        let index_data: &[u8] = unsafe { slice::from_raw_parts(indices.as_ptr() as *const u8, indices.len() * size_of::<i32>()) };
        let index_buffer = device.create_buffer_init(&BufferInitDescriptor {
            usage: BufferUsages::INDEX,
            contents: index_data,
            label: None,
        });

        let inner = MeshInner {
            _vertex_buffer: vertex_buffer,
            _index_buffer: index_buffer,
            renderer: renderer.clone(),
            pipeline: render_pipeline,
        };

        drop(lock);
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub fn downgrade(&self) -> WeakMesh {
        WeakMesh(Arc::downgrade(&self.inner))
    }
}