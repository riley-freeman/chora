use std::slice;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::sync::Weak;
use wgpu::{Buffer, BufferUsages, Device};
use wgpu::util::BufferInitDescriptor;
use wgpu::util::DeviceExt;
use cgmath::Matrix4;
use crate::render_pipeline::RenderPipeline;
use crate::Renderer;

pub(crate) struct MeshInner {
    pub(crate) _vertex_buffer: wgpu::Buffer,
    pub(crate) _index_buffer: wgpu::Buffer,

    renderer: Renderer,
    pipeline: RenderPipeline,

    // Weak reference to parent model (if this mesh is part of a model)
    parent_model: Option<WeakModel>,
}

// Forward declaration - defined in model.rs
use crate::model::ModelInner;

#[derive(Clone)]
pub struct WeakModel(pub(crate) Weak<ModelInner>);

impl Drop for MeshInner {
    fn drop(&mut self) {

    }
}

pub struct Mesh {
    pub(crate) inner: Arc<Mutex<MeshInner>>,
    pub(crate) added: AtomicBool,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WeakMesh(Weak<Mutex<MeshInner>>);

impl WeakMesh {
    pub(crate) fn upgrade(&self) -> Option<Arc<Mutex<MeshInner>>> {
        self.0.upgrade()
    }
}

impl Drop for Mesh {
    fn drop(&mut self) {
        if !self.added.load(Ordering::Relaxed) { return; }
        let renderer = {
            let lock = self.inner.lock().unwrap();
            lock.renderer.clone()
        };
        renderer.remove_mesh_from_render_queue(&self);
    }
}

impl Mesh {
    pub fn new(renderer: Renderer, device: &Device, vertices: &[f32], indices: &[i32], render_pipeline: RenderPipeline) -> Self {
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
            parent_model: None,
        };

        Self {
            inner: Arc::new(Mutex::new(inner)),
            added: AtomicBool::new(false),
        }
    }

    pub fn render_pipeline(&self) -> RenderPipeline {
        let lock = self.inner.lock().unwrap();
        lock.pipeline.clone()
    }

    pub fn downgrade(&self) -> WeakMesh {
        WeakMesh(Arc::downgrade(&self.inner))
    }

    /// Set the parent model for this mesh (internal use)
    pub(crate) fn set_parent_model(&self, model: WeakModel) {
        let mut lock = self.inner.lock().unwrap();
        lock.parent_model = Some(model);
    }

    /// Get the model matrix from the parent model, if available
    ///
    /// Returns None if this mesh has no parent model or if the model has been dropped
    pub fn model_matrix(&self) -> Option<Matrix4<f32>> {
        let lock = self.inner.lock().unwrap();

        if let Some(weak_model) = &lock.parent_model {
            if let Some(model_inner) = weak_model.0.upgrade() {
                return Some(*model_inner.model_matrix.lock().unwrap());
            }
        }

        None
    }

    /// Get the model's GPU buffer, if available
    ///
    /// Returns None if this mesh has no parent model or if the model has been dropped
    pub fn model_buffer(&self) -> Option<Buffer> {
        let lock = self.inner.lock().unwrap();

        if let Some(weak_model) = &lock.parent_model {
            if let Some(model_inner) = weak_model.0.upgrade() {
                return Some(model_inner.model_buffer.clone());
            }
        }

        None
    }
}

