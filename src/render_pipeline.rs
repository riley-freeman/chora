use crate::coordination::TextureRemapping;
use crate::model::ModelBufferStruct;
use crate::render_target::RenderTarget;
use crate::sampler::Sampler;
use crate::shader_parser::ParsedShader;
use crate::shader_rewriter::ShaderRewriter;
use crate::texture::Texture;
use bitflags::bitflags;
use bytemuck::offset_of;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::{Arc, Weak};
use wgpu::MultisampleState;
use wgpu::PipelineLayoutDescriptor;
use wgpu::PrimitiveState;
use wgpu::RenderPipelineDescriptor;
use wgpu::ShaderModuleDescriptor;
use wgpu::VertexState;
use wgpu::{BindGroup, BindGroupLayout, Device, FragmentState, TextureView};

bitflags! {
    #[derive(Default, PartialEq, Eq, Clone, Copy, Debug)]
    pub struct RenderPipelineFlags: u32 {
        const ALLOW_WORLD_UNIFORM = 1;
        const ALLOW_CAMERA_UNIFORM = 2;
        const ALLOW_OBJECT_UNIFORM = 4;

        const OVERRIDE_VERTEX_INPUT = 8;
    }
}

pub(crate) struct RenderPipelineInner {
    pub(crate) textures: Vec<Texture>,
    pub(crate) sampler: Option<Sampler>,
    pub(crate) original_shader_source: String,
    pub(crate) shader_code: String,

    pub(crate) _flags: RenderPipelineFlags,

    _uniform_bind_group_layout: wgpu::BindGroupLayout,
    _texture_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) _texture_bind_group: wgpu::BindGroup,

    _pipeline_layout: wgpu::PipelineLayout,
    pub(crate) _render_pipeline: wgpu::RenderPipeline,
    pub(crate) texture_bind_group_index: u32,
}

#[derive(Clone)]
pub struct RenderPipeline {
    pub(crate) inner: Arc<Mutex<RenderPipelineInner>>,
    pub(crate) flags: RenderPipelineFlags,
}

#[derive(Debug, Clone)]
pub struct WeakRenderPipeline {
    inner: Weak<Mutex<RenderPipelineInner>>,
    flags: RenderPipelineFlags,
}

impl RenderPipeline {
    pub fn new(
        device: &wgpu::Device,
        render_target: &dyn RenderTarget,
        bind_group_layout: &BindGroupLayout,
        source: &str,
        textures: &[Texture],
        sampler: Option<Sampler>,
        remappings: Vec<TextureRemapping>,
        flags: RenderPipelineFlags,
    ) -> Self {
        // Parse the shader
        let parsed_shader = ParsedShader::parse(source);

        // Build texture index map (texture_name -> global_texture_index)
        let mut texture_index_map = HashMap::new();
        for (tex_name, _) in &parsed_shader.texture_references {
            // For now, map each texture to its own index
            // This assumes texture order matches usage order
            let index = texture_index_map.len() as u32;
            texture_index_map.insert(tex_name.clone(), index);
        }

        // Build atlas binding map (global_texture_index -> atlas_binding_index)
        // For single-texture pipelines, all textures map to atlas 0
        let mut atlas_binding_map = HashMap::new();
        for i in 0..textures.len() {
            atlas_binding_map.insert(i as u32, 0);
        }

        // Rewrite the shader using the shader rewriter
        let mut shader = ShaderRewriter::rewrite_shader(
            &parsed_shader,
            flags,
            &texture_index_map,
            &atlas_binding_map,
        );

        // println!("SHADER: \n{}", shader);

        // Generate textureSample functions for each remapped texture
        // let mut generated_functions = String::new();
        // for remapping in &remappings {
        //     let function_name = format!("textureSample{}", remapping.binding);
        //     let min_x = remapping.coords.min().x;
        //     let min_y = remapping.coords.min().y;
        //     let max_x = remapping.coords.max().x;
        //     let max_y = remapping.coords.max().y;

        //     let function = format!(
        //         "fn {}(tex: texture_2d<f32>, samp: sampler, uv: vec2<f32>) -> vec4<f32> {{\n\
        //          \tlet min_uv = vec2<f32>({}, {});\n\
        //          \tlet max_uv = vec2<f32>({}, {});\n\
        //          \tlet remapped_uv = min_uv + uv * (max_uv - min_uv);\n\
        //          \treturn textureSample(tex, samp, remapped_uv);\n\
        //          }}\n\n",
        //         function_name, min_x, min_y, max_x, max_y
        //     );
        //     generated_functions.push_str(&function);

        //     // Replace the binding number in the shader source
        //     // Match patterns like "@binding(44)" and replace with "@binding(0)"
        //     let old_binding = format!("@binding({})", remapping.binding);
        //     let new_binding = format!("@binding({})", remapping.new_binding);
        //     shader = shader.replace(&old_binding, &new_binding);
        // }

        // // Prepend generated functions to the shader source
        // let shader = format!("{}{}", generated_functions, shader);

        // Get the flag info (about the uniform buffers)
        let allow_object_uniform = flags.contains(RenderPipelineFlags::ALLOW_OBJECT_UNIFORM);

        // Create the uniform bind group layout.
        let uniform_bind_group_layout = bind_group_layout.clone();

        // Create the texture bind group layout.
        let texture_bind_group_layout =
            Self::create_texture_bind_group_layout(device, textures, &sampler);

        let texture_views = textures
            .iter()
            .map(|texture| texture.view())
            .collect::<Vec<_>>();
        let texture_bind_group = Self::create_texture_bind_group(
            device,
            textures,
            &sampler,
            &texture_bind_group_layout,
            &texture_views,
        );

        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(shader.clone().into()),
        });

        let mut bind_group_layouts = Vec::new();
        let mut texture_bind_group_index = 0;
        if !(flags & !RenderPipelineFlags::OVERRIDE_VERTEX_INPUT).is_empty() {
            bind_group_layouts.push(&uniform_bind_group_layout);
            texture_bind_group_index = 1;
        }
        bind_group_layouts.push(&texture_bind_group_layout);

        let desc = PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &bind_group_layouts,
            push_constant_ranges: &[],
        };
        let pipeline_layout = device.create_pipeline_layout(&desc);

        let color_target_states = render_target.color_target_states();
        let depth_target_state = render_target.depth_stencil_state();

        let mut vertex_state_buffers = Vec::with_capacity(2);
        let per_instance_buffer_layout_attributes = create_per_instance_vertex_attributes();
        let per_vertex_buffer_layout_attributes = create_per_vertex_attributes();

        vertex_state_buffers.push(create_per_vertex_buffer_layout(
            &per_vertex_buffer_layout_attributes,
        ));
        if allow_object_uniform {
            vertex_state_buffers.push(create_per_instance_buffer_layout(
                &per_instance_buffer_layout_attributes,
            ));
        }

        let desc = RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader_module,
                compilation_options: Default::default(),
                entry_point: Some("vs_main"),
                buffers: &vertex_state_buffers,
            },
            #[allow(unused)]
            fragment: Some(FragmentState {
                module: &shader_module,
                compilation_options: Default::default(),
                entry_point: Some("fs_main"),
                targets: color_target_states.as_ref(),
            }),
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: depth_target_state,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        };
        let render_pipeline = device.create_render_pipeline(&desc);

        let inner = RenderPipelineInner {
            textures: Vec::from(textures),
            sampler,
            original_shader_source: source.to_string(),
            shader_code: shader,
            _uniform_bind_group_layout: uniform_bind_group_layout,
            _texture_bind_group_layout: texture_bind_group_layout,
            _texture_bind_group: texture_bind_group,
            _pipeline_layout: pipeline_layout,
            _render_pipeline: render_pipeline,
            texture_bind_group_index,
            _flags: flags,
        };

        Self {
            inner: Arc::new(Mutex::new(inner)),
            flags,
        }
    }

    fn create_texture_bind_group(
        device: &Device,
        textures: &[Texture],
        sampler: &Option<Sampler>,
        texture_bind_group_layout: &BindGroupLayout,
        texture_views: &Vec<TextureView>,
    ) -> BindGroup {
        let hal_sampler = sampler
            .as_ref()
            .map(|s| s.inner.lock().unwrap().sampler.clone());

        let mut texture_bind_group_entries = Vec::with_capacity(textures.len());
        for i in 0..textures.len() {
            texture_bind_group_entries.push(wgpu::BindGroupEntry {
                binding: i as _,
                resource: wgpu::BindingResource::TextureView(&texture_views[i]),
            });
        }
        if let Some(sampler) = &hal_sampler {
            let binding = textures.len() as _;
            texture_bind_group_entries.push(wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::Sampler(sampler),
            })
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &texture_bind_group_layout,
            entries: &texture_bind_group_entries,
        })
    }

    fn create_texture_bind_group_layout(
        device: &Device,
        textures: &[Texture],
        sampler: &Option<Sampler>,
    ) -> BindGroupLayout {
        let mut texture_bind_group_layout_entries = Vec::with_capacity(textures.len());

        for i in 0..textures.len() {
            texture_bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding: i as _,
                count: None,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
            });
        }

        if sampler.is_some() {
            let binding = textures.len() as _;
            texture_bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding,
                count: None,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            });
        }

        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &texture_bind_group_layout_entries,
        })
    }

    pub fn downgrade(&self) -> WeakRenderPipeline {
        WeakRenderPipeline {
            inner: Arc::downgrade(&self.inner),
            flags: self.flags.clone(),
        }
    }

    pub fn textures(&self) -> Vec<Texture> {
        let lock = self.inner.lock().unwrap();
        lock.textures.clone()
    }

    pub fn sampler(&self) -> Option<Sampler> {
        let lock = self.inner.lock().unwrap();
        lock.sampler.clone()
    }

    pub fn shader_code(&self) -> String {
        let lock = self.inner.lock().unwrap();
        lock.shader_code.clone()
    }

    pub fn original_shader_source(&self) -> String {
        let lock = self.inner.lock().unwrap();
        lock.original_shader_source.clone()
    }
}

fn create_per_vertex_attributes() -> Vec<wgpu::VertexAttribute> {
    vec![
        // Position
        wgpu::VertexAttribute {
            shader_location: 0,
            offset: 0,
            format: wgpu::VertexFormat::Float32x3,
        },
        // Texture Coordinates
        wgpu::VertexAttribute {
            shader_location: 1,
            offset: 12,
            format: wgpu::VertexFormat::Float32x2,
        },
        // // Color
        // wgpu::VertexAttribute {
        //     shader_location: 2,
        //     offset: 12,
        //     format: wgpu::VertexFormat::Float32x3,
        // },
    ]
}

fn create_per_vertex_buffer_layout(
    attributes: &[wgpu::VertexAttribute],
) -> wgpu::VertexBufferLayout<'_> {
    wgpu::VertexBufferLayout {
        array_stride: 20,
        // array_stride: 24,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: attributes,
    }
}

fn create_per_instance_vertex_attributes() -> Vec<wgpu::VertexAttribute> {
    vec![
        wgpu::VertexAttribute {
            shader_location: 4,
            offset: offset_of!(ModelBufferStruct, model_matrix) as _,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            shader_location: 5,
            offset: (offset_of!(ModelBufferStruct, model_matrix) + 16) as _,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            shader_location: 6,
            offset: (offset_of!(ModelBufferStruct, model_matrix) + 32) as _,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            shader_location: 7,
            offset: (offset_of!(ModelBufferStruct, model_matrix) + 48) as _,
            format: wgpu::VertexFormat::Float32x4,
        },
    ]
}

fn create_per_instance_buffer_layout(
    attributes: &[wgpu::VertexAttribute],
) -> wgpu::VertexBufferLayout<'_> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<ModelBufferStruct>() as _,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: attributes,
    }
}

impl WeakRenderPipeline {
    pub fn upgrade(&self) -> Option<RenderPipeline> {
        self.inner.upgrade().map(|inner| RenderPipeline {
            inner,
            flags: self.flags.clone(),
        })
    }
}
