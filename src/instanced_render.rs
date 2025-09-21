use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;
use etagere::{size2, Allocation, AtlasAllocator};
use crate::{Renderer, MAX_TEXTURE_SIZE};
use crate::linked_list::LinkedList;
use crate::mesh::WeakMesh;
use crate::render_pipeline::{RenderPipeline, WeakRenderPipeline};
use crate::texture::{Texture, TextureInner};


pub struct InstancedRender {
    renderer: Renderer,

    mesh_collection: LinkedList<WeakMesh>,
    atlas_collection: Mutex<LinkedList<*const TextureInner>>,
    instanced_renders: HashMap<*const c_void, crate::InstancedRender>,

    render_pipeline: WeakRenderPipeline,
    texture_mappings: HashMap<*const TextureInner, Sprite>,
}

enum Spritesheet {
    Etagere {
        allocator: AtlasAllocator,
        texture: Texture,
    },
    MaxRects {
        texture: Texture,
    },
}

#[derive(Clone)]
struct Sprite {
    spritesheet: Texture,
    scissor: wgpu::Extent3d,
}

impl InstancedRender {
    pub fn add_mesh(&self, mesh: WeakMesh) {

    }

    fn add_render_pipeline(&self, pipeline: RenderPipeline) {

    }

    fn add_texture(&self, texture: Texture) -> Sprite {
        todo!()
    }
}