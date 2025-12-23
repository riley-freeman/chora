use crate::coordination::TextureRemapping;
use crate::linked_list::LinkedList;
use crate::mesh::{Mesh, WeakMesh};
use crate::render_pipeline::{RenderPipeline, RenderPipelineFlags};
use crate::shader_parser::ParsedShader;
use crate::shader_rewriter::ShaderRewriter;
use crate::shader_merger::ShaderMerger;
use crate::texture::{Spritesheet, Texture, TextureInner};
use crate::{Renderer, RendererInner};
use std::collections::HashMap;
use std::sync::Mutex;

pub struct InstancedRender {
    _renderer: Renderer,
    render_pipelines: Vec<RenderPipeline>,
    spritesheet: Spritesheet,

    // Multi-shader batching
    shader_variants: Vec<String>,           // Original shader sources
    shader_variant_map: HashMap<String, u32>, // shader_source -> variant_id
    merged_pipeline: Option<RenderPipeline>, // The mega-pipeline with all variants
    instance_shader_ids: Vec<u32>,          // shader_id for each instance

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

            shader_variants: Vec::new(),
            shader_variant_map: HashMap::new(),
            merged_pipeline: None,
            instance_shader_ids: Vec::new(),

            count: 0,

            _mesh: mesh.downgrade(),
            _mesh_collection: LinkedList::new(),
            _atlas_collection: Default::default(),
        }
    }

    pub fn add_mesh(&mut self, r_inner: &RendererInner, mesh: &Mesh) {
        let mesh_rp = mesh.render_pipeline();
        let shader_source = mesh_rp.original_shader_source();

        // Register this shader variant
        let shader_id = self.register_shader_variant(shader_source);

        // Store shader_id for this instance
        self.instance_shader_ids.push(shader_id);

        // Add textures to spritesheet
        let mesh_textures = mesh_rp.textures();
        let _sprites = self.spritesheet.add_textures_locked(r_inner, &mesh_textures);

        // If we have multiple shader variants, regenerate the merged pipeline
        if self.shader_variants.len() > 1 && self.merged_pipeline.is_none() {
            self.regenerate_merged_pipeline(r_inner);
        } else if self.shader_variants.len() == 1 && self.render_pipelines.is_empty() {
            // First mesh - create a simple pipeline (no batching yet)
            self.create_simple_pipeline(r_inner, mesh);
        } else if self.shader_variants.len() > 1 {
            // We already have a merged pipeline, just regenerate to include new textures
            self.regenerate_merged_pipeline(r_inner);
        }

        self.count += 1;
    }

    /// Register a shader variant and return its ID
    fn register_shader_variant(&mut self, shader_source: String) -> u32 {
        if let Some(&variant_id) = self.shader_variant_map.get(&shader_source) {
            return variant_id;
        }

        let variant_id = self.shader_variants.len() as u32;
        self.shader_variants.push(shader_source.clone());
        self.shader_variant_map.insert(shader_source, variant_id);
        variant_id
    }

    /// Create a simple pipeline for single shader (no batching)
    fn create_simple_pipeline(&mut self, r_inner: &RendererInner, mesh: &Mesh) {
        let mesh_rp = mesh.render_pipeline();
        let shader = mesh_rp.original_shader_source();
        let sampler = mesh_rp.sampler();
        let mesh_textures = mesh_rp.textures();
        let sprites = self.spritesheet.add_textures_locked(r_inner, &mesh_textures);

        // Build atlas mapping
        let (atlases, atlas_map) = self.build_atlas_mapping(&sprites);

        // Create texture remappings
        let remappings = self.create_texture_remappings(&sprites, &atlas_map);

        let pipeline = RenderPipeline::new(
            &r_inner.device,
            &r_inner.camera,
            &shader,
            &atlases,
            sampler,
            remappings,
            RenderPipelineFlags::default(),
        );

        self.render_pipelines.push(pipeline);
    }

    /// Regenerate the merged pipeline with all shader variants
    fn regenerate_merged_pipeline(&mut self, r_inner: &RendererInner) {
        // 1. Collect all unique textures from all shader variants
        let all_textures = self.collect_all_textures();

        // 2. Pack into atlases
        let sprites = self.spritesheet.add_textures_locked(r_inner, &all_textures);

        // 3. Build atlas mapping
        let (atlases, atlas_map) = self.build_atlas_mapping(&sprites);

        // 4. Create texture remappings
        let remappings = self.create_texture_remappings(&sprites, &atlas_map);

        // 5. Parse all shader variants
        let parsed_shaders: Vec<ParsedShader> = self.shader_variants
            .iter()
            .map(|src| ParsedShader::parse(src))
            .collect();

        // 6. Build texture index map (texture_name -> global_index)
        let texture_index_map = self.build_texture_index_map(&all_textures, &parsed_shaders);

        // 7. Build atlas binding map (global_texture_index -> atlas_binding)
        let atlas_binding_map = self.build_atlas_binding_map(&sprites, &atlas_map);

        // 8. Rewrite each shader to use textureSample#
        let first_rp = self.render_pipelines.first().unwrap();
        let rewritten_shaders: Vec<String> = parsed_shaders
            .iter()
            .map(|s| ShaderRewriter::rewrite_shader(s, first_rp.flags, &texture_index_map, &atlas_binding_map))
            .collect();

        // 9. Merge into mega-shader
        let merged_shader = ShaderMerger::merge_shaders(
            &rewritten_shaders,
            &parsed_shaders,
            &remappings,
            atlases.len(),
        );

        // 10. Create new render pipeline with merged shader
        let sampler = if let Some(rp) = self.render_pipelines.first() {
            rp.sampler()
        } else {
            None
        };

        let new_pipeline = RenderPipeline::new(
            &r_inner.device,
            &r_inner.camera,
            &merged_shader,
            &atlases,
            sampler,
            remappings,
            RenderPipelineFlags::default(),
        );

        self.merged_pipeline = Some(new_pipeline);
    }

    /// Collect all unique textures from all shader variants
    fn collect_all_textures(&self) -> Vec<Texture> {
        // For now, just collect from existing pipelines
        // TODO: Parse shader sources to find texture references
        let mut all_textures = Vec::new();
        for pipeline in &self.render_pipelines {
            all_textures.extend(pipeline.textures());
        }
        all_textures
    }

    /// Build mapping of textures to atlas indices
    fn build_atlas_mapping(&self, sprites: &[crate::texture::Sprite]) -> (Vec<Texture>, HashMap<Texture, usize>) {
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

    /// Create texture remappings from sprites
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
                TextureRemapping::from_sprite(
                    original_binding as u32,
                    *atlas_idx as u32,
                    sprite,
                )
            })
            .collect()
    }

    /// Build texture name to global index mapping
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

    /// Build global texture index to atlas binding mapping
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

    pub fn remove_mesh(&mut self, _mesh: &Mesh) {
        self.count -= 1;
    }

    pub fn mesh_count(&self) -> usize {
        self.count
    }
}
