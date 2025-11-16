use crate::coordination::TextureRemapping;
use crate::linked_list::LinkedList;
use crate::mesh::{Mesh, WeakMesh};
use crate::render_pipeline::{RenderPipeline, RenderPipelineFlags};
use crate::texture::{Spritesheet, Texture, TextureInner};
use crate::{Renderer, RendererInner};
use std::collections::HashMap;
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

        // Build atlas texture list and create texture->atlas mapping
        let mut atlas_map: HashMap<Texture, usize> = HashMap::new();
        let mut atlases = Vec::new();

        for sprite in &sprites {
            let atlas = sprite.atlas_texture().clone();
            if !atlas_map.contains_key(&atlas) {
                let idx = atlases.len();
                atlases.push(atlas.clone());
                atlas_map.insert(atlas, idx);
            }
        }

        // Try to add to an existing render pipeline
        for p in &mut self.render_pipelines {
            // GPUs have a max bindable texture count.
            let lock = p.lock();
            if lock.textures.len() + atlases.len() > 16 {
                continue;
            }

            // Create a new render pipeline with remapping
            let shader = lock.original_shader_source.clone();
            let sampler = lock.sampler.clone();

            let existing_texture_count = lock.textures.len();
            let mut textures = lock.textures.clone();
            textures.extend(atlases.clone());

            // Create texture remappings for each original texture
            let mut remappings = Vec::new();
            for (original_binding, sprite) in sprites.iter().enumerate() {
                let atlas = sprite.atlas_texture();
                let atlas_idx = atlas_map.get(atlas).unwrap();
                let new_binding = existing_texture_count + atlas_idx;

                remappings.push(TextureRemapping::from_sprite(
                    original_binding as u32,
                    new_binding as u32,
                    sprite,
                ));
            }

            let new_rp = RenderPipeline::new(
                &r_inner.device,
                &r_inner.camera,
                &shader,
                &textures,
                sampler,
                remappings,
                RenderPipelineFlags::default(),
            );

            // Update the render pipeline
            drop(lock);
            *p = new_rp;
            self.count += 1;
            return;
        }

        // Create a new render pipeline with remapping
        let shader = mesh_rp.original_shader_source();
        let sampler = mesh_rp.sampler();

        // Create texture remappings for each original texture
        let mut remappings = Vec::new();
        for (original_binding, sprite) in sprites.iter().enumerate() {
            let atlas = sprite.atlas_texture();
            let atlas_idx = atlas_map.get(atlas).unwrap();

            remappings.push(TextureRemapping::from_sprite(
                original_binding as u32,
                *atlas_idx as u32,
                sprite,
            ));
        }

        let new_rp = RenderPipeline::new(
            &r_inner.device,
            &r_inner.camera,
            &shader,
            &atlases,
            sampler,
            remappings,
            RenderPipelineFlags::default(),
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
