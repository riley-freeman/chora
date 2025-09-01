use cgmath::{Matrix4, Vector3};
use wgpu::{Buffer, BufferUsages};
use wgpu::Device;
use wgpu::wgt::BufferDescriptor;
use crate::mesh::Mesh;

use std::mem;

pub struct Model {
    mutable: bool,
    meshes: Vec<Mesh>,

    position: *const Vector3<f32>,
    rotation: *const Vector3<f32>,
    scale   : *const Vector3<f32>,

    model_buffer: Buffer,
}

struct ModelBufferStruct {
    model_matrix: Matrix4<f32>
}

impl Model {
    pub fn new(device: &Device, meshes: Vec<Mesh>, mutable: bool,
               position: *const f32, rotation: *const f32, scale: *const f32) -> Self
    {
        // Create a buffer for the model's render data
        // Keep this a 4x4 matrix just for future simplicity (maybe...)
        let model_buffer = device.create_buffer(&BufferDescriptor {
            size: mem::size_of::<ModelBufferStruct>() as _,
            usage: BufferUsages::UNIFORM,
            mapped_at_creation: false,
            label: None,
        });

        Model {
            mutable,
            meshes,

            position: position as _,
            rotation: rotation as _,
            scale   : scale as _,

            model_buffer,
        }

    }
}

