use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};

use image::DynamicImage;
use image::ImageReader;
use image::ColorType;

use wgpu::{BindGroupDescriptor, BindGroupEntry, BindGroupLayout, ColorWrites, Device, Extent3d, Queue, TextureView};
use wgpu::TextureDescriptor;
use wgpu::TextureDimension;
use wgpu::TextureFormat;
use wgpu::wgt::TextureDataOrder;
use wgpu::util::DeviceExt;
use crate::render_target::RenderTarget;
use crate::Renderer;

pub(crate) struct TextureInner {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    _renderer: Renderer,
    bind_group: wgpu::BindGroup,

    width: u32,
    height: u32,
}

impl TextureInner {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

#[derive(Clone)]
pub struct Texture {
    pub(crate) inner: Arc<TextureInner>,
    format: TextureFormat,
}

#[derive(Debug, Clone)]
pub struct WeakTexture(Weak<Mutex<TextureInner>>);

impl Texture {
    pub fn new(renderer: Renderer, device: &Device, queue: &Queue, cast_bind_group_layout: &BindGroupLayout, cast_sampler: &wgpu::Sampler, width: u32, height: u32, format: TextureFormat, data: Option<&[u8]>) -> Self {
        let desc = create_new_texture_desc(width, height, format);

        let texture = match data {
            Some(data) => {
                 device.create_texture_with_data(
                    queue,
                    &desc,
                    TextureDataOrder::LayerMajor,
                    data,
                 )
            }
            None => {
                device.create_texture(&desc)
            }
        };

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: cast_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(cast_sampler),
                }
            ]
        });

        let inner = TextureInner {
            _texture: texture,
            bind_group,
            view,
            width,
            height,
            _renderer: renderer,
        };

        Self {
            inner: Arc::new(inner),
            format,
        }
    }

    pub fn empty(renderer: Renderer, device: &Device, cast_bind_group_layout: &BindGroupLayout, cast_sampler: &wgpu::Sampler, width: u32, height: u32, format: TextureFormat) -> Self {
        let desc = create_new_texture_desc(width, height, format);
        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: cast_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(cast_sampler),
                }
            ]
        });
        let inner = TextureInner {
            _texture: texture,
            bind_group,
            view,
            width,
            height,
            _renderer: renderer,
        };

        Self {
            inner: Arc::new(inner),
            format,
        }
    }

    pub fn load_from_file(renderer: Renderer, device: &Device, queue: &Queue, cast_bind_group_layout: &BindGroupLayout, cast_sampler: &wgpu::Sampler, path: &Path) -> io::Result<Self> {
        let mut img = ImageReader::open(path)?.decode().unwrap();

        let width = img.width();
        let height = img.height();

        let format = match img.color() {
            ColorType::L8 => TextureFormat::R8Unorm,
            ColorType::La8 => TextureFormat::Rg8Unorm,
            ColorType::L16 => TextureFormat::R16Unorm,
            ColorType::Rgba8 => TextureFormat::Rgba8Unorm,
            ColorType::La16 => TextureFormat::Rg16Unorm,
            ColorType::Rgba16 => TextureFormat::Rgba16Unorm,
            ColorType::Rgba32F => TextureFormat::Rgba32Float,

            ColorType::Rgb8 => {
                img = DynamicImage::ImageRgba8(img.to_rgba8());
                TextureFormat::Rgba8Unorm
            }
            ColorType::Rgb16 => {
                img = DynamicImage::ImageRgba16(img.to_rgba16());
                TextureFormat::Rgba16Unorm
            }
            ColorType::Rgb32F => {
                img = DynamicImage::ImageRgba32F(img.to_rgba32f());
                TextureFormat::Rgba32Float
            }

            #[allow(unreachable_patterns)]
            ColorType::L16 => TextureFormat::R16Unorm,

            _ => unimplemented!(),
        };
        let data = img.as_bytes();

        Ok(Texture::new(renderer, device, queue, cast_bind_group_layout, cast_sampler, width, height, format, Some(data)))
    }

    pub fn width(&self) -> u32 {
        self.inner.width()
    }

    pub fn height(&self) -> u32 {
        self.inner.height()
    }

    pub fn view(&self) -> TextureView { self.inner.view.clone() }

    pub fn bind_group(&self) -> wgpu::BindGroup {
        self.inner.bind_group.clone()
    }
}

impl RenderTarget for Texture {
    fn color_target_states(&self) -> Vec<Option<wgpu::ColorTargetState>> {
        if !self.format.is_depth_stencil_format() {
            vec![Some(wgpu::ColorTargetState {
                format: self.format,
                write_mask: ColorWrites::all(),
                blend: None,
            })]
        } else {
            vec![]
        }
    }

    fn depth_stencil_state(&self) -> Option<wgpu::DepthStencilState> {
        if self.format.is_depth_stencil_format() {
            Some (wgpu::DepthStencilState {
                depth_write_enabled: true,
                format: self.format,
                stencil: wgpu::StencilState::default(),
                depth_compare: wgpu::CompareFunction::Less,
                bias: wgpu::DepthBiasState::default(),
            })
        } else {
            None
        }
    }
}

fn create_new_texture_desc<'a>(width: u32, height: u32, format: TextureFormat) -> TextureDescriptor<'a> {
    TextureDescriptor {
        label: None,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        format,
        dimension: TextureDimension::D2,
        sample_count: 1,
        mip_level_count: 1,
        view_formats: &[]
    }
}