use crate::Model;
use crate::render_pipeline::RenderPipeline;
use crate::{Renderer, WeakRenderer};
use cgmath::Matrix4;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Weak;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{mem, slice};
use wgpu::util::BufferInitDescriptor;
use wgpu::util::DeviceExt;
use wgpu::{Buffer, BufferUsages, Device};

pub(crate) struct MeshInner {
    pub(crate) _vertex_buffer: wgpu::Buffer,
    pub(crate) _index_buffer: wgpu::Buffer,

    renderer: WeakRenderer,
    pipeline: RenderPipeline,
}

// Forward declaration - defined in model.rs
use crate::model::ModelInner;

#[derive(Clone, Debug)]
pub struct WeakModel(pub(crate) Weak<ModelInner>);

impl Drop for MeshInner {
    fn drop(&mut self) {}
}

pub struct Mesh {
    pub(crate) inner: Arc<Mutex<MeshInner>>,
    pub(crate) added: AtomicBool,
    // Per-handle parent model reference; clones get their own independent slot.
    pub(crate) parent_model: Mutex<Option<WeakModel>>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WeakMesh {
    pub(crate) inner: Weak<Mutex<MeshInner>>,
    pub(crate) model: Option<WeakModel>,
}

impl WeakMesh {
    pub(crate) fn upgrade(&self) -> Option<Mesh> {
        if let Some(inner) = self.inner.upgrade() {
            Some(Mesh {
                inner,
                added: AtomicBool::new(false),
                parent_model: Mutex::new(self.model.clone()),
            })
        } else {
            None
        }
    }
}

impl Drop for Mesh {
    fn drop(&mut self) {
        if !self.added.load(Ordering::Relaxed) {
            return;
        }
        let lock = self.inner.lock().unwrap();
        if let Some(renderer) = lock.renderer.upgrade() {
            std::mem::drop(lock);
            renderer.remove_mesh_from_render_queue(&self);
        }
    }
}

impl Mesh {
    pub fn new(
        renderer: Renderer,
        device: &Device,
        vertices: &[f32],
        indices: &[i32],
        render_pipeline: RenderPipeline,
    ) -> Self {
        let vertex_data: &[u8] = unsafe {
            slice::from_raw_parts(
                vertices.as_ptr() as *const u8,
                vertices.len() * size_of::<f32>(),
            )
        };

        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            usage: BufferUsages::VERTEX,
            contents: vertex_data,
            label: None,
        });

        let index_data: &[u8] = unsafe {
            slice::from_raw_parts(
                indices.as_ptr() as *const u8,
                indices.len() * size_of::<i32>(),
            )
        };
        let index_buffer = device.create_buffer_init(&BufferInitDescriptor {
            usage: BufferUsages::INDEX,
            contents: index_data,
            label: None,
        });

        let inner = MeshInner {
            _vertex_buffer: vertex_buffer,
            _index_buffer: index_buffer,
            renderer: renderer.downgrade(),
            pipeline: render_pipeline,
        };

        Self {
            inner: Arc::new(Mutex::new(inner)),
            added: AtomicBool::new(false),
            parent_model: Mutex::new(None),
        }
    }

    pub fn renderer(&self) -> Option<Renderer> {
        self.inner.lock().unwrap().renderer.upgrade()
    }

    pub fn render_pipeline(&self) -> RenderPipeline {
        let lock = self.inner.lock().unwrap();
        lock.pipeline.clone()
    }

    /// Snapshot the current parent_model into the WeakMesh so the renderer can
    /// retrieve the correct per-handle model matrix even when many clones share
    /// the same underlying GPU buffers.
    pub fn downgrade(&self) -> WeakMesh {
        let model = self.parent_model.lock().unwrap().clone();
        WeakMesh {
            inner: Arc::downgrade(&self.inner),
            model,
        }
    }

    /// Set the parent model for this mesh handle (internal use).
    pub(crate) fn set_parent_model(&self, model: WeakModel) {
        *self.parent_model.lock().unwrap() = Some(model);
    }

    pub fn get_parent_model(&self) -> Option<Model> {
        let lock = self.parent_model.lock().unwrap();
        let weak = lock.as_ref()?;
        unsafe { mem::transmute(weak.0.upgrade()?) }
    }

    pub fn model_matrix(&self) -> Option<Matrix4<f32>> {
        let lock = self.parent_model.lock().unwrap();
        let weak_model = lock.as_ref()?;
        let model_inner = weak_model.0.upgrade()?;
        Some(model_inner.model_buffer_info.lock().unwrap().model_matrix)
    }

    pub fn model_buffer(&self, variant: usize) -> Option<Buffer> {
        let lock = self.parent_model.lock().unwrap();
        let weak_model = lock.as_ref()?;
        let model_inner = weak_model.0.upgrade()?;
        Some(model_inner.model_buffers[variant].clone())
    }
}

impl Clone for Mesh {
    fn clone(&self) -> Self {
        Mesh {
            inner: Arc::clone(&self.inner),
            added: AtomicBool::new(false),
            parent_model: Mutex::new(None), // each clone starts with its own empty model slot
        }
    }
}
