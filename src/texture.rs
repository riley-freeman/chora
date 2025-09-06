use std::io;
use std::path::Path;
use std::sync::Arc;

use image::{DynamicImage, EncodableLayout};
use image::ImageReader;
use image::ColorType;

use wgpu::Extent3d;
use wgpu::TextureDescriptor;
use wgpu::TextureDimension;
use wgpu::TextureFormat;
use wgpu::wgt::TextureDataOrder;
use wgpu::util::DeviceExt;

use crate::Renderer;

struct TextureInner {
    texture: wgpu::Texture,
    renderer: Renderer,
}

#[derive(Clone)]
pub struct Texture {
    inner: Arc<TextureInner>,
}

impl Texture {
    pub fn new(renderer: Renderer, width: u32, height: u32, format: TextureFormat, data: Option<&[u8]>) -> Self {
        let desc = create_new_texture_desc(width, height, format);

        let lock = renderer.0.lock().unwrap();
        let device = &lock.device;
        let queue = &lock.queue;

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

        drop(lock);

        let inner = TextureInner {
            texture,
            renderer,
        };

        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn load_from_file(renderer: Renderer, path: &Path) -> io::Result<Self> {
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

        Ok(Texture::new(renderer, width, height, format, Some(data)))
    }
}

fn create_new_texture_desc<'a>(width: u32, height: u32, format: TextureFormat) -> TextureDescriptor<'a> {
    TextureDescriptor {
        label: None,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
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