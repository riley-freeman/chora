use std::slice;
use std::sync::{Arc, Mutex, Weak};
use wgpu::BufferUsages;
use wgpu::util::BufferInitDescriptor;
use wgpu::util::DeviceExt;

pub(crate) struct MeshInner {
    _vertex_buffer: wgpu::Buffer,
    _index_buffer: wgpu::Buffer,

    // TODO: Add some shader or pipeline or something to this...
}


pub struct Mesh(pub(crate) Arc<Mutex<MeshInner>>);
#[derive(Debug, Clone)]
pub struct WeakMesh(pub(crate) Weak<Mutex<MeshInner>>);

impl Mesh {
    pub fn new(device: &wgpu::Device, vertices: &[f32], indices: &[i32]) -> Self {
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

        let inner = MeshInner { _vertex_buffer: vertex_buffer, _index_buffer: index_buffer };
        Self(Arc::new(Mutex::new(inner)))
    }

    pub fn downgrade(&self) -> WeakMesh {
        WeakMesh(Arc::downgrade(&self.0))
    }
}