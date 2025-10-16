use crate::linked_list::LinkedList;
use crate::mesh::{Mesh, WeakMesh};
use crate::render_pipeline::RenderPipeline;
use crate::texture::{Spritesheet, TextureInner};
use crate::{Renderer, RendererInner};
use std::collections::HashSet;
use std::sync::Mutex;

pub struct InstancedRender {
    _renderer: Renderer,
    render_pipelines: Vec<RenderPipeline>,
    spritesheet: Spritesheet,

    // TODO: replace this member with something a little more useful
    count: usize,

    _mesh: WeakMesh,
    _mesh_collection: LinkedList<Mesh>,
    _atlas_collection: Mutex<LinkedList<*const TextureInner>>,
}
impl InstancedRender {
    pub fn new(
        renderer: Renderer,
        r_inner: &RendererInner,
        mesh: &Mesh,
    ) -> Self {
        Self {
            _renderer: renderer.clone(),
            render_pipelines: Default::default(),
            spritesheet: Spritesheet::new_lock(
                renderer,
                r_inner
            ),

            count: 0,

            _mesh: mesh.downgrade(),
            _mesh_collection: LinkedList::new(),
            _atlas_collection: Default::default(),
        }
    }

    pub fn add_mesh(&mut self, r_inner: &RendererInner, mesh: &Mesh) {
        let mesh_rp = mesh.render_pipeline();

        let mesh_textures = mesh_rp.textures();
        let sprites = self.spritesheet.add_textures_locked(r_inner, &mesh_textures);

        let atlases = sprites
            .iter()
            .map(|sprite| sprite.spritesheet.clone())
            .collect::<HashSet<_>>();

        // Try to add to an existing render pipeline
        for p in &mut self.render_pipelines {
            // GPUs have a max bindable texture count.
            let lock = p.lock();
            if lock.textures.len() + atlases.len() > 16 {
                continue;
            }

            // Create a new render pipeline
            let shader = lock.shader_code.clone();
            let sampler = lock.sampler.clone();

            let mut textures = lock.textures.clone();
            textures.extend(atlases.clone());

            let new_rp = RenderPipeline::new(
                &r_inner.device,
                &r_inner.camera,
                &shader,
                &textures,
                sampler,
                false,
                false,
                false,
            );

            // Update the render pipeline
            drop(lock);
            *p = new_rp;
            self.count += 1;
            return;
        }

        // Create a new render pipeline
        let shader = mesh_rp.shader_code().clone();
        let sampler = mesh_rp.sampler().clone();
        let textures = atlases.iter().cloned().collect::<Vec<_>>();

        let new_rp = RenderPipeline::new(
            &r_inner.device,
            &r_inner.camera,
            &shader,
            &textures,
            sampler,
            false,
            false,
            false,
        );
        self.render_pipelines.push(new_rp);
        self.count += 1;
    }

    pub fn remove_mesh(&mut self, _mesh: &Mesh) {
        self.count -= 1;
    }

    pub fn mesh_count(&self) -> usize {
        self.count
    }
}
