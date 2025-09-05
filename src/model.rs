use cgmath::{Matrix4, Vector3};
use wgpu::{Buffer, BufferUsages};
use wgpu::Device;
use wgpu::wgt::BufferDescriptor;
use crate::mesh::Mesh;

use std::mem;
use std::sync::Arc;

struct ModelInner {
    _mutable: bool,
    meshes: Vec<Mesh>,

    _position: *const Vector3<f32>,
    _rotation: *const Vector3<f32>,
    _scale: *const Vector3<f32>,

    _model_buffer: Buffer,
}

pub struct Model(Arc<ModelInner>);



struct ModelBufferStruct {
    _model_matrix: Matrix4<f32>
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

        let inner = ModelInner {
            _mutable: mutable,
            meshes,

            _position: position as _,
            _rotation: rotation as _,
            _scale: scale as _,

            _model_buffer: model_buffer,
        };

        Self(Arc::new(inner))
    }

    /// Returns an iterator over references to the meshes in this model
    pub fn iter(&self) -> MeshIter<'_> {
        MeshIter {
            meshes: &self.0.meshes,
            index: 0,
        }
    }
}

impl<'a> IntoIterator for &'a Model {
    type Item = &'a Mesh;
    type IntoIter = MeshIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        MeshIter {
            meshes: &self.0.meshes,
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

