use std::slice;
use wgpu::BufferUsages;
use wgpu::util::BufferInitDescriptor;
use wgpu::util::DeviceExt;

pub struct Mesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,

    // TODO: Add some shader or pipeline or something to this...
}

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

        Mesh { vertex_buffer, index_buffer }
    }
}