use std::sync::{Arc, MutexGuard, Weak};
use std::sync::Mutex;

use wgpu::{BindGroup, BindGroupLayout, Device, FragmentState, TextureView};
use wgpu::MultisampleState;
use wgpu::PipelineLayoutDescriptor;
use wgpu::PrimitiveState;
use wgpu::RenderPipelineDescriptor;
use wgpu::ShaderModuleDescriptor;
use wgpu::VertexState;
use crate::render_target::RenderTarget;
use crate::sampler::Sampler;
use crate::texture::Texture;

pub(crate) struct RenderPipelineInner {
    pub(crate) textures: Vec<Texture>,
    pub(crate) sampler: Option<Sampler>,
    pub(crate) shader_code: String,

    _uniform_bind_group_layout: wgpu::BindGroupLayout,
    _texture_bind_group_layout: wgpu::BindGroupLayout,
    _texture_bind_group: wgpu::BindGroup,

    _pipeline_layout: wgpu::PipelineLayout,
    _render_pipeline: wgpu::RenderPipeline,
}

#[derive(Clone)]
pub struct RenderPipeline {
    pub(crate) inner: Arc<Mutex<RenderPipelineInner>>,
}

#[derive(Debug, Clone)]
pub struct WeakRenderPipeline(Weak<Mutex<RenderPipelineInner>>);

impl RenderPipeline {
    pub fn new(
        device: &wgpu::Device,
        render_target: &dyn RenderTarget,
        shader: &str,
        textures: &[Texture],
        sampler: Option<Sampler>,
        allow_world_uniform: bool,
        allow_camera_uniform: bool,
        allow_object_uniform: bool,
    ) -> Self {
        // Create the uniform bind group layout.
        let uniform_bind_group_layout = Self::create_uniform_bind_group_layout(device, allow_world_uniform, allow_camera_uniform, allow_object_uniform);

        // Create the texture bind group layout.
        let texture_bind_group_layout = Self::create_texture_bind_group_layout(device, textures, &sampler);

        let texture_views = textures.iter().map(|texture| texture.view()).collect::<Vec<_>>();
        let texture_bind_group = Self::create_texture_bind_group(device, textures, &sampler, &texture_bind_group_layout, &texture_views);

        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(shader.into()),
        });

        let desc = PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&uniform_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        };
        let pipeline_layout = device.create_pipeline_layout(&desc);

        let color_target_states = render_target.color_target_states();
        let depth_target_state = render_target.depth_stencil_state();

        let desc = RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader_module,
                compilation_options: Default::default(),
                entry_point: Some("vs_main"),
                buffers: &[],
            },
            #[allow(unused)]
            fragment: Some(FragmentState {
                module: &shader_module,
                compilation_options: Default::default(),
                entry_point: Some("fs_main"),
                targets: color_target_states.as_ref(),
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: depth_target_state,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        };
        let render_pipeline = device.create_render_pipeline(&desc);

        let inner = RenderPipelineInner {
            textures: Vec::from(textures),
            sampler,
            shader_code: shader.into(),
            _uniform_bind_group_layout: uniform_bind_group_layout,
            _texture_bind_group_layout: texture_bind_group_layout,
            _texture_bind_group: texture_bind_group,
            _pipeline_layout: pipeline_layout,
            _render_pipeline: render_pipeline,
        };

        Self {
            inner: Arc::new(Mutex::new(inner))
        }
    }

    fn create_texture_bind_group(device: &Device, textures: &[Texture], sampler: &Option<Sampler>, texture_bind_group_layout: &BindGroupLayout, texture_views: &Vec<TextureView>) -> BindGroup {
        let hal_sampler = sampler.as_ref().map(|s| {
            s.inner.lock().unwrap().sampler.clone()
        });

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
                resource: wgpu::BindingResource::Sampler(sampler)
            })
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &texture_bind_group_layout,
            entries: &texture_bind_group_entries
        })
    }

    fn create_texture_bind_group_layout(device: &Device, textures: &[Texture], sampler: &Option<Sampler>) -> BindGroupLayout {
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
                }
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

    fn create_uniform_bind_group_layout(device: &Device, allow_world_uniform: bool, allow_camera_uniform: bool, allow_object_uniform: bool) -> BindGroupLayout {
        let mut uniform_bind_group_layout_entries = Vec::new();
        if allow_world_uniform {
            uniform_bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding: 0,
                count: None,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            });
        }
        if allow_camera_uniform {
            uniform_bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding: 1,
                count: None,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            });
        }
        if allow_object_uniform {
            uniform_bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding: 2,
                count: None,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            });
        }
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &uniform_bind_group_layout_entries,
        })
    }

    pub fn downgrade(&self) -> WeakRenderPipeline {
        WeakRenderPipeline(Arc::downgrade(&self.inner))
    }

    pub (crate) fn lock(&'_ self) -> MutexGuard<'_, RenderPipelineInner> {
        self.inner.lock().unwrap()
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
}

impl WeakRenderPipeline {
    pub fn upgrade(&self) -> Option<RenderPipeline> {
        self.0.upgrade().map(|inner| RenderPipeline { inner })
    }
}


