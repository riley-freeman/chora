use crate::coordination::TextureRemapping;
use crate::linked_list::LinkedList;
use crate::mesh::{Mesh, WeakMesh};
use crate::model::{Model, ModelBufferStruct};
use crate::render_pipeline::{RenderPipeline, RenderPipelineFlags};
use crate::shader_merger::ShaderMerger;
use crate::shader_parser::ParsedShader;
use crate::shader_rewriter::ShaderRewriter;
use crate::texture::{Spritesheet, Texture, TextureInner};
use crate::{Renderer, RendererInner};
use cgmath::Matrix4;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

pub(crate) struct InstancedRenderInner {
    render_pipelines: Vec<RenderPipeline>,
    spritesheet: Spritesheet,

    // Multi-shader batching
    shader_variants: Vec<String>,
    shader_variant_map: HashMap<String, u32>,
    merged_pipeline: Option<RenderPipeline>,
    instance_shader_ids: Vec<u32>,

    // GPU buffer holding per-instance model matrices, one slot per frame-in-flight
    instance_buffers: Vec<Option<Arc<wgpu::Buffer>>>,
    instance_buffer_capacity: usize,
    num_buffers: usize,

    count: usize,

    _mesh: WeakMesh,
    mesh_collection: LinkedList<WeakMesh>,
    _atlas_collection: Mutex<LinkedList<*const TextureInner>>,
}

unsafe impl Send for InstancedRenderInner {}
unsafe impl Sync for InstancedRenderInner {}

#[derive(Clone)]
pub struct InstancedRender {
    pub(crate) inner: Arc<Mutex<InstancedRenderInner>>,
}

#[derive(Clone, Default)]
pub struct WeakInstancedRender {
    pub(crate) inner: Weak<Mutex<InstancedRenderInner>>,
}

impl InstancedRenderInner {
    fn new(renderer: Renderer, r_inner: &RendererInner, mesh: &Mesh, num_buffers: usize) -> Self {
        Self {
            render_pipelines: Default::default(),
            spritesheet: Spritesheet::new_lock(renderer, r_inner),

            shader_variants: Vec::new(),
            shader_variant_map: HashMap::new(),
            merged_pipeline: None,
            instance_shader_ids: Vec::new(),

            instance_buffers: vec![None; num_buffers],
            instance_buffer_capacity: 0,
            num_buffers,

            count: 0,

            _mesh: mesh.downgrade(),
            mesh_collection: LinkedList::new(),
            _atlas_collection: Default::default(),
        }
    }

    fn active_pipeline(&self) -> Option<&RenderPipeline> {
        self.merged_pipeline
            .as_ref()
            .or_else(|| self.render_pipelines.first())
    }

    pub fn update_instance_buffer(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        buffer: usize,
        offset: usize,
        matrices: &[Matrix4<f32>],
    ) {
        let count = matrices.len();
        if count == 0 {
            return;
        }

        let needed_size = (count * std::mem::size_of::<ModelBufferStruct>()) as u64;

        if self.instance_buffer_capacity < count {
            for slot in &mut self.instance_buffers {
                *slot = Some(Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Instance Buffer"),
                    size: needed_size,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })));
            }
            self.instance_buffer_capacity = count;
        } else if self.instance_buffers[buffer].is_none() {
            self.instance_buffers[buffer] = Some(Arc::new(device.create_buffer(
                &wgpu::BufferDescriptor {
                    label: Some("Instance Buffer"),
                    size: needed_size,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                },
            )));
        }

        let data: Vec<ModelBufferStruct> = matrices
            .iter()
            .map(|m| ModelBufferStruct { model_matrix: *m })
            .collect();

        if let Some(buf) = &self.instance_buffers[buffer] {
            let byte_offset = (offset * std::mem::size_of::<ModelBufferStruct>()) as u64;
            queue.write_buffer(buf, byte_offset, bytemuck::cast_slice(&data));
        }
    }

    fn instance_buffer(&self, buffer: usize) -> Option<&Arc<wgpu::Buffer>> {
        self.instance_buffers[buffer].as_ref()
    }

    fn add_mesh(&mut self, r_inner: &RendererInner, mesh: &Mesh, weak_self: WeakInstancedRender) {
        let mesh_rp = mesh.render_pipeline();
        let shader_source = mesh_rp.original_shader_source();

        let shader_id = self.register_shader_variant(shader_source);
        self.instance_shader_ids.push(shader_id);

        let mesh_textures = mesh_rp.textures();
        let _sprites = self
            .spritesheet
            .add_textures_locked(r_inner, &mesh_textures);

        if self.shader_variants.len() > 1 && self.merged_pipeline.is_none() {
            self.regenerate_merged_pipeline(r_inner);
        } else if self.shader_variants.len() == 1 && self.render_pipelines.is_empty() {
            self.create_simple_pipeline(r_inner, mesh);
        } else if self.shader_variants.len() > 1 {
            self.regenerate_merged_pipeline(r_inner);
        }
        self.mesh_collection.push_back(mesh.downgrade());

        let device = &r_inner.device;
        let queue = &r_inner.queue;

        let matrices: Vec<Matrix4<f32>> = self
            .mesh_collection
            .iter()
            .enumerate()
            .filter_map(|(i, weak_mesh)| {
                let m = weak_mesh.upgrade()?;
                Some(
                    if let Some(model) = m.parent_model.lock().unwrap().as_ref().and_then(|wm| {
                        let inner = wm.0.upgrade()?;
                        Some(unsafe {
                            std::mem::transmute::<Arc<crate::model::ModelInner>, Model>(inner)
                        })
                    }) {
                        // Find which slot in the model's mesh list this handle corresponds to.
                        let j = model
                            .inner
                            .meshes
                            .iter()
                            .position(|mesh_in_model| Arc::ptr_eq(&mesh_in_model.inner, &m.inner))
                            .unwrap_or(0);
                        model.set_instanced_render(j, weak_self.clone(), i);
                        model.update_matrix();
                        model.model_matrix()
                    } else {
                        Matrix4::from_scale(1.0)
                    },
                )
            })
            .collect();

        for slot in 0..self.num_buffers {
            self.update_instance_buffer(device, queue, slot, 0, &matrices);
        }
        self.count += 1;
    }

    fn register_shader_variant(&mut self, shader_source: String) -> u32 {
        if let Some(&variant_id) = self.shader_variant_map.get(&shader_source) {
            return variant_id;
        }
        let variant_id = self.shader_variants.len() as u32;
        self.shader_variants.push(shader_source.clone());
        self.shader_variant_map.insert(shader_source, variant_id);
        variant_id
    }

    fn create_simple_pipeline(&mut self, r_inner: &RendererInner, mesh: &Mesh) {
        let mesh_rp = mesh.render_pipeline();
        let shader = mesh_rp.original_shader_source();
        let sampler = mesh_rp.sampler();
        let flags = mesh_rp.flags;
        let mesh_textures = mesh_rp.textures();
        let sprites = self
            .spritesheet
            .add_textures_locked(r_inner, &mesh_textures);

        let (atlases, atlas_map) = self.build_atlas_mapping(&sprites);
        let remappings = self.create_texture_remappings(&sprites, &atlas_map);

        let pipeline = RenderPipeline::new(
            &r_inner.device,
            &r_inner.camera,
            &r_inner.camera_bind_group_layout,
            &shader,
            &atlases,
            sampler,
            remappings,
            flags,
        );

        self.render_pipelines.push(pipeline);
    }

    fn regenerate_merged_pipeline(&mut self, r_inner: &RendererInner) {
        let all_textures = self.collect_all_textures();
        let sprites = self.spritesheet.add_textures_locked(r_inner, &all_textures);
        let (atlases, atlas_map) = self.build_atlas_mapping(&sprites);
        let remappings = self.create_texture_remappings(&sprites, &atlas_map);

        let parsed_shaders: Vec<ParsedShader> = self
            .shader_variants
            .iter()
            .map(|src| ParsedShader::parse(src))
            .collect();

        let texture_index_map = self.build_texture_index_map(&all_textures, &parsed_shaders);
        let atlas_binding_map = self.build_atlas_binding_map(&sprites, &atlas_map);

        let first_rp = self.render_pipelines.first().unwrap();
        let rewritten_shaders: Vec<String> = parsed_shaders
            .iter()
            .map(|s| {
                ShaderRewriter::rewrite_shader(
                    s,
                    first_rp.flags,
                    &texture_index_map,
                    &atlas_binding_map,
                )
            })
            .collect();

        let merged_shader = ShaderMerger::merge_shaders(
            &rewritten_shaders,
            &parsed_shaders,
            &remappings,
            atlases.len(),
        );

        let sampler = self.render_pipelines.first().and_then(|rp| rp.sampler());

        let new_pipeline = RenderPipeline::new(
            &r_inner.device,
            &r_inner.camera,
            &r_inner.camera_bind_group_layout,
            &merged_shader,
            &atlases,
            sampler,
            remappings,
            RenderPipelineFlags::default(),
        );

        self.merged_pipeline = Some(new_pipeline);
    }

    fn collect_all_textures(&self) -> Vec<Texture> {
        let mut all_textures = Vec::new();
        for pipeline in &self.render_pipelines {
            all_textures.extend(pipeline.textures());
        }
        all_textures
    }

    fn build_atlas_mapping(
        &self,
        sprites: &[crate::texture::Sprite],
    ) -> (Vec<Texture>, HashMap<Texture, usize>) {
        let mut atlas_map = HashMap::new();
        let mut atlases = Vec::new();
        for sprite in sprites {
            let atlas = sprite.atlas_texture().clone();
            if !atlas_map.contains_key(&atlas) {
                let idx = atlases.len();
                atlases.push(atlas.clone());
                atlas_map.insert(atlas, idx);
            }
        }
        (atlases, atlas_map)
    }

    fn create_texture_remappings(
        &self,
        sprites: &[crate::texture::Sprite],
        atlas_map: &HashMap<Texture, usize>,
    ) -> Vec<TextureRemapping> {
        sprites
            .iter()
            .enumerate()
            .map(|(original_binding, sprite)| {
                let atlas = sprite.atlas_texture();
                let atlas_idx = atlas_map.get(atlas).unwrap();
                TextureRemapping::from_sprite(original_binding as u32, *atlas_idx as u32, sprite)
            })
            .collect()
    }

    fn build_texture_index_map(
        &self,
        _all_textures: &[Texture],
        parsed_shaders: &[ParsedShader],
    ) -> HashMap<String, u32> {
        let mut map = HashMap::new();
        let mut index = 0u32;
        for shader in parsed_shaders {
            for tex_name in shader.texture_names() {
                if !map.contains_key(tex_name) {
                    map.insert(tex_name.clone(), index);
                    index += 1;
                }
            }
        }
        map
    }

    fn build_atlas_binding_map(
        &self,
        sprites: &[crate::texture::Sprite],
        atlas_map: &HashMap<Texture, usize>,
    ) -> HashMap<u32, u32> {
        sprites
            .iter()
            .enumerate()
            .map(|(tex_index, sprite)| {
                let atlas = sprite.atlas_texture();
                let atlas_binding = *atlas_map.get(atlas).unwrap() as u32;
                (tex_index as u32, atlas_binding)
            })
            .collect()
    }

    fn remove_mesh(&mut self, _mesh: &Mesh) {
        self.count -= 1;
    }

    fn mesh_count(&self) -> usize {
        self.count
    }
}

impl InstancedRender {
    pub fn new(renderer: Renderer, r_inner: &RendererInner, mesh: &Mesh, num_buffers: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(InstancedRenderInner::new(
                renderer, r_inner, mesh, num_buffers,
            ))),
        }
    }

    pub fn active_pipeline(&self) -> Option<RenderPipeline> {
        self.inner.lock().unwrap().active_pipeline().cloned()
    }

    pub fn instance_buffer(&self, buffer: usize) -> Option<Arc<wgpu::Buffer>> {
        self.inner.lock().unwrap().instance_buffer(buffer).cloned()
    }

    pub fn add_mesh(&self, r_inner: &RendererInner, mesh: &Mesh) {
        let weak_self = self.downgrade();
        self.inner
            .lock()
            .unwrap()
            .add_mesh(r_inner, mesh, weak_self);
    }

    pub fn remove_mesh(&self, mesh: &Mesh) {
        self.inner.lock().unwrap().remove_mesh(mesh);
    }

    pub fn mesh_count(&self) -> usize {
        self.inner.lock().unwrap().mesh_count()
    }

    pub fn update_instance_buffer(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        buffer: usize,
        offset: usize,
        matrices: &[Matrix4<f32>],
    ) {
        self.inner
            .lock()
            .unwrap()
            .update_instance_buffer(device, queue, buffer, offset, matrices)
    }

    pub fn downgrade(&self) -> WeakInstancedRender {
        WeakInstancedRender {
            inner: Arc::downgrade(&self.inner),
        }
    }
}

impl WeakInstancedRender {
    pub fn upgrade(&self) -> Option<InstancedRender> {
        self.inner.upgrade().map(|inner| InstancedRender { inner })
    }
}
