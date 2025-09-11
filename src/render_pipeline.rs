use std::sync::{Arc, Weak};
use std::sync::Mutex;

use wgpu::FragmentState;
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
    textures: Vec<Texture>,
    sampler: Option<Sampler>,
    shader_code: String,

    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group: wgpu::BindGroup,

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
        sampler: Option<Sampler>
    ) -> Self {
        // Create the texture bind group layout.
        let mut texture_bind_group_layout_entries = Vec::with_capacity(textures.len());
        let mut texture_bind_group_entries = Vec::with_capacity(textures.len());

        let texture_views = textures.iter().map(|texture| texture.view()).collect::<Vec<_>>();

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

            texture_bind_group_entries.push(wgpu::BindGroupEntry {
                binding: i as _,
                resource: wgpu::BindingResource::TextureView(&texture_views[i]),
            });
        }

        // Add the sampler if it exists.
        let hal_sampler = sampler.as_ref().map(|s| {
            s.inner.lock().unwrap().sampler.clone()
        });

        // Add it to the bind group.
        if let Some(sampler) = &hal_sampler {
            let binding = textures.len() as _;
            texture_bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding,
                count: None,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            });

            texture_bind_group_entries.push(wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::Sampler(sampler)
            })
        }

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &texture_bind_group_layout_entries,
            });

        let texture_bind_group =
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &texture_bind_group_layout,
                entries: &texture_bind_group_entries
            });

        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(shader.into()),
        });

        let desc = PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&texture_bind_group_layout],
            push_constant_ranges: &[],
        };
        let pipeline_layout = device.create_pipeline_layout(&desc);

        let color_target_states = render_target.color_target_states();

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
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        };
        let render_pipeline = device.create_render_pipeline(&desc);

        let inner = RenderPipelineInner {
            textures: Vec::from(textures),
            sampler,
            shader_code: shader.into(),
            texture_bind_group_layout,
            texture_bind_group,
            _pipeline_layout: pipeline_layout,
            _render_pipeline: render_pipeline,
        };

        Self {
            inner: Arc::new(Mutex::new(inner))
        }
    }

    pub fn downgrade(&self) -> WeakRenderPipeline {
        WeakRenderPipeline(Arc::downgrade(&self.inner))
    }

    pub fn textures<'a>(&self) -> Vec<Texture> {
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


