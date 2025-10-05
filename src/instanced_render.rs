use crate::Renderer;
use crate::camera::Camera;
use crate::linked_list::LinkedList;
use crate::mesh::{Mesh, WeakMesh};
use crate::render_pipeline::RenderPipeline;
use crate::texture::{Spritesheet, TextureInner};
use std::collections::HashSet;
use std::sync::Mutex;
use wgpu::{BindGroupLayout, Device, Queue};

pub struct InstancedRender {
    renderer: Renderer,
    render_pipelines: Vec<RenderPipeline>,
    spritesheet: Spritesheet,

    // TODO: replace this member with something a little more useful
    count: usize,

    mesh: WeakMesh,
    mesh_collection: LinkedList<Mesh>,
    atlas_collection: Mutex<LinkedList<*const TextureInner>>,
}
impl InstancedRender {
    pub fn new(
        renderer: Renderer,
        device: &Device,
        queue: &Queue,
        cast_bind_group_layout: &BindGroupLayout,
        cast_render_pipeline: &wgpu::RenderPipeline,
        cast_sampler: &wgpu::Sampler,
        mesh: &Mesh,
    ) -> Self {
        Self {
            renderer: renderer.clone(),
            render_pipelines: Default::default(),
            spritesheet: Spritesheet::new(
                renderer,
                device,
                queue,
                cast_bind_group_layout,
                cast_render_pipeline,
                cast_sampler,
            ),

            count: 0,

            mesh: mesh.downgrade(),
            mesh_collection: LinkedList::new(),
            atlas_collection: Default::default(),
        }
    }

    pub fn add_mesh(&mut self, device: &Device, camera: &Camera, mesh: &Mesh) {
        let mesh_rp = mesh.render_pipeline();

        let mesh_textures = mesh_rp.textures();
        let sprites = self.spritesheet.add_textures(&mesh_textures);

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
                device, camera, &shader, &textures, sampler, false, false, false,
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
            device, camera, &shader, &textures, sampler, false, false, false,
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
