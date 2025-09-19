use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};
use cgmath::num_traits::FromPrimitive;
use image::{DynamicImage, GenericImageView};
use image::ImageReader;
use image::ColorType;

use rayon::prelude::*;

use wgpu::{BindGroupDescriptor, BindGroupEntry, BindGroupLayout, Buffer, BufferAddress, BufferUsages, ColorWrites, Device, Extent3d, Origin3d, Queue, Sampler, SubmissionIndex, TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect, TextureView};
use wgpu::TextureDescriptor;
use wgpu::TextureDimension;
use wgpu::TextureFormat;
use wgpu::wgt::{PollType, TextureDataOrder};
use wgpu::util::DeviceExt;
use crate::render_target::RenderTarget;
use crate::Renderer;

pub(crate) struct TextureInner {
    texture: wgpu::Texture,
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
                    TextureDataOrder::MipMajor,
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
            texture: texture,
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
            texture: texture,
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
        let img = ImageReader::open(path)?.decode().unwrap();

        let width = img.width();
        let height = img.height();
        let (format, data) = Self::align_image_data(img);
        Ok(Texture::new(renderer, device, queue, cast_bind_group_layout, cast_sampler, width, height, format, Some(&data)))
    }

    fn align_image_data(mut img: DynamicImage) -> (TextureFormat, Vec<u8>) {
        let format = match img.color() {
            ColorType::L8 => TextureFormat::R8Unorm,
            ColorType::La8 => TextureFormat::Rg8Unorm,
            ColorType::L16 => TextureFormat::R16Unorm,
            ColorType::Rgba8 => TextureFormat::Rgba8Unorm,
            ColorType::La16 => TextureFormat::Rg16Unorm,
            ColorType::Rgba16 => TextureFormat::Rgba16Unorm,
            ColorType::Rgba32F => TextureFormat::Rgba32Float,

            ColorType::Rgb8 => {
                img = DynamicImage::ImageRgba8(Self::rgb_to_rgba_parallel(img).unwrap());
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

        (format, img.into_bytes())
    }

    fn rgb_to_rgba_parallel(rgb: DynamicImage) -> image::ImageResult<image::RgbaImage> {
        let (w, h) = rgb.dimensions();
        let src = rgb.into_bytes();

        let row_sz_src = (w as usize) * 3;
        let row_sz_dst = (w as usize) * 4;
        let mut dst = vec![0u8; (w as usize) * (h as usize) * 4];

        // process each row in parallel without capturing &mut dst
        // by zipping parallel row slices
        src.par_chunks(row_sz_src)
            .zip(dst.par_chunks_mut(row_sz_dst))
            .for_each(|(src_row, dst_row)| {
                let mut s_i = 0;
                let mut d_i = 0;
                while s_i < src_row.len() {
                    // copy 3 bytes
                    dst_row[d_i    ] = src_row[s_i    ];
                    dst_row[d_i + 1] = src_row[s_i + 1];
                    dst_row[d_i + 2] = src_row[s_i + 2];
                    dst_row[d_i + 3] = 255; // alpha

                    s_i += 3;
                    d_i += 4;
                }
            });

        Ok(image::RgbaImage::from_raw(w, h, dst).expect("size ok"))
    }

    pub fn width(&self) -> u32 {
        self.inner.width()
    }

    pub fn height(&self) -> u32 {
        self.inner.height()
    }

    pub fn view(&self) -> TextureView { self.inner.view.clone() }

    pub fn format(&self) -> TextureFormat {
        self.format
    }

    pub fn bind_group(&self) -> wgpu::BindGroup {
        self.inner.bind_group.clone()
    }

    pub fn write_to_new_buffer(&self, device: &Device, queue: &Queue) -> wgpu::Buffer {
        let width = self.width();
        let height = self.height();
        let bytes_per_pixel = self.format
            .block_copy_size(None)
            .unwrap();

        let size = width * height * bytes_per_pixel;
        let desc = wgpu::BufferDescriptor {
            label: None,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            size: BufferAddress::from(size),
            mapped_at_creation: false,
        };
        let buffer = device.create_buffer(&desc);
        let texture = &self.inner.texture;

        let _ = Self::cmd_write_to_buffer(device, queue, width, height, bytes_per_pixel, texture, &buffer);

        buffer
    }

    fn cmd_write_to_buffer(
        device: &Device,
        queue: &Queue,
        width: u32,
        height: u32,
        bytes_per_pixel: u32,
        src: &wgpu::Texture,
        dest: &wgpu::Buffer
    ) -> SubmissionIndex {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let source = TexelCopyTextureInfo {
            texture: src,
            aspect: TextureAspect::All,
            mip_level: 0,
            origin: Origin3d::ZERO,
        };
        let destination = TexelCopyBufferInfo {
            buffer: &dest,
            layout: TexelCopyBufferLayout {
                bytes_per_row: Some(width * bytes_per_pixel),
                rows_per_image: Some(height),
                offset: BufferAddress::from_i32(0).unwrap(),
            }
        };
        encoder.copy_texture_to_buffer(
            source,
            destination,
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            }
        );

        queue.submit(Some(encoder.finish()))
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
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
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