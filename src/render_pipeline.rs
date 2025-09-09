use std::ffi::c_void;
use std::sync::{Arc, Weak};
use std::sync::Mutex;

use wgpu::{ColorTargetState, FragmentState, TextureFormat};
use wgpu::MultisampleState;
use wgpu::PipelineLayoutDescriptor;
use wgpu::PrimitiveState;
use wgpu::RenderPipelineDescriptor;
use wgpu::ShaderModuleDescriptor;
use wgpu::VertexState;
use crate::mesh::MeshInner;
use crate::texture::Texture;

struct RenderPipelineInner {
    textures: Vec<Texture>,
    shader_code: String,
    pipeline_layout: wgpu::PipelineLayout,
    render_pipeline: wgpu::RenderPipeline,
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
        shader: &str,
        textures: &[Texture]
    ) -> Self {
        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(shader.into()),
        });

        let desc = PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        };
        let pipeline_layout = device.create_pipeline_layout(&desc);

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
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })]    // todo: Camera's texture format
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
            shader_code: shader.into(),
            pipeline_layout,
            render_pipeline,
        };

        Self {
            inner: Arc::new(Mutex::new(inner))
        }
    }

    pub fn textures<'a>(&self) -> Vec<Texture> {
        let lock = self.inner.lock().unwrap();
        lock.textures.clone()
    }

    pub fn shader_code(&self) -> String {
        let lock = self.inner.lock().unwrap();
        lock.shader_code.clone()
    }
}


