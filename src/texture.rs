use std::collections::HashMap;
use std::io;
use std::mem::transmute;
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};
use cgmath::num_traits::FromPrimitive;
use etagere::{size2, AllocId, Allocation, AtlasAllocator, Rectangle};
use etagere::euclid::{Point2D, Rect};
use image::{DynamicImage, GenericImageView};
use image::ImageReader;
use image::ColorType;
use max_rects::bucket::Bucket;
use max_rects::calculate_packed_percentage;
use max_rects::max_rects::MaxRects;
use max_rects::packing_box::PackingBox;
use rayon::prelude::*;

use wgpu::{BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BufferAddress, BufferUsages, ColorWrites, Device, Extent3d, LoadOp, Operations, Origin3d, Queue, StoreOp, SubmissionIndex, TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect, TextureView};
use wgpu::TextureDescriptor;
use wgpu::TextureDimension;
use wgpu::TextureFormat;
use wgpu::wgt::{TextureDataOrder};
use wgpu::util::DeviceExt;
use crate::render_target::RenderTarget;
use crate::{HousedTexture, Renderer, MAX_TEXTURE_SIZE};
use crate::linked_list::LinkedList;

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

pub struct Spritesheet {
    inner: Arc<Mutex<SpritesheetInner>>,
}

struct SpritesheetInner {
    pub(crate) textures: Vec<Texture>,
    pub(crate) shelf_allocators: Vec<AtlasAllocator>,

    pub(crate) texture_mappings: HashMap<*const TextureInner, (usize, Allocation)>,

    renderer: Renderer,
}

pub struct Sprite {
    pub(crate) spritesheet: Texture,
    pub(crate) scissor: Rectangle,
}

impl Spritesheet {
    pub fn new(renderer: Renderer) -> Self {
        let inner = SpritesheetInner {
            textures: Vec::new(),
            shelf_allocators: Vec::new(),
            texture_mappings: HashMap::new(),
            renderer
        };
        Self {
            inner: Arc::new(Mutex::new(inner))
        }
    }

    pub fn add_texture(&self, texture: &[Texture]) -> Vec<Sprite> {
        let mut res = Vec::with_capacity(texture.len());
        for t in texture {
            let sprite = self.allocate(&t);

            let lock = self.inner.lock().unwrap();
            let renderer = lock.renderer.0.lock().unwrap();

            let texture_addr = sprite.spritesheet.inner.as_ref();

            cast_texture_to_atlas(
                &renderer.device,
                &renderer.queue,
                &renderer.cast_render_pipeline,
                texture_addr,
                &sprite.spritesheet,
                &sprite.scissor
            );
            res.push(sprite);
        }


        // self.convert_to_max_rects();

        res
    }

    fn allocate(&self, texture: &Texture) -> Sprite {
        let texture_address = texture.inner.as_ref() as *const _;

        let mut lock = self.inner.lock().unwrap();
        if let Some((index, scissor)) = lock.texture_mappings.get(&texture_address) {
            return Sprite {
                spritesheet: lock.textures[*index].clone(),
                scissor: scissor.rectangle,
            }
        }

        let width = texture.width();
        let height = texture.height();

        for (i, allocator) in lock.shelf_allocators.iter_mut().enumerate() {
            let alloc = allocator.allocate(size2(
                width as _, height as _
            ));

            if let Some(alloc) = alloc {
                lock.texture_mappings.insert(texture_address, (i, alloc));
                return Sprite {
                    spritesheet: lock.textures[i].clone(),
                    scissor: alloc.rectangle,
                };
            }
        }


        let renderer = &lock.renderer;

        let spritesheet = renderer.create_texture(
            MAX_TEXTURE_SIZE,
            MAX_TEXTURE_SIZE,
            TextureFormat::Rgba8Unorm,
            None,
        );

        let mut allocator = AtlasAllocator::new(size2(
            MAX_TEXTURE_SIZE as _,
            MAX_TEXTURE_SIZE as _,
        ));

        let alloc = allocator.allocate(size2(
            width as _, height as _,
        )).unwrap();

        let texture_len = lock.textures.len();
        lock.textures.push(spritesheet.clone());
        lock.shelf_allocators.push(allocator);

        lock.texture_mappings.insert(texture_address, (texture_len, alloc));



        Sprite {
            spritesheet,
            scissor: alloc.rectangle,
        }
    }

    fn convert_to_max_rects(&self) {
        // TODO: MAKE THIS ASYNC
        let mut lock = self.inner.lock().unwrap();

        let mut boxes = Vec::with_capacity(lock.texture_mappings.len());
        let mut textures = HashMap::new();
        for (addr, _) in lock.texture_mappings.iter() {
            let texture_inner = unsafe { &*(*addr) };
            let width = texture_inner.width();
            let height = texture_inner.height();
            boxes.push(PackingBox::new(
                width as _,
                height as _,
            ));
            textures.entry((width, height)).or_insert(LinkedList::new());
            textures.get_mut(&(width, height)).unwrap().push_back(texture_inner);
        }

        let pp = calculate_packed_percentage(&boxes, &[Bucket::new(
            MAX_TEXTURE_SIZE as _,
            MAX_TEXTURE_SIZE as _,
            0, 0, 1
        )]);

        // Initialize the first bins via estimating how much over we go.
        let mut bins = Vec::with_capacity((pp / 100.0) as usize);
        for i in 0..bins.capacity() {
            Self::push_new_bin(&mut bins, i);
        }

        let mut i = bins.len();
        let placed = loop {
            Self::push_new_bin(&mut bins, i);

            let mut problem = MaxRects::new(boxes.clone(), bins.clone());
            let (placed, remaining, _) = problem.place();
            if remaining.is_empty() {
                break placed;
            }

            i += 1;
        };

        // Create the Textures
        let mut atlases = Vec::with_capacity(bins.len());
        for _ in 0..bins.len() {
            let sheet = lock.renderer.create_texture(
                MAX_TEXTURE_SIZE,
                MAX_TEXTURE_SIZE,
                TextureFormat::Rgba8Unorm,
                None,
            );
            atlases.push(sheet);
        }

        lock.texture_mappings.clear();
        let renderer = lock.renderer.0.lock().unwrap();
        let device = renderer.device.clone();
        let queue = renderer.queue.clone();
        let cast_render_pipeline = renderer.cast_render_pipeline.clone();
        drop(renderer);

        for (i, alloc) in placed.iter().enumerate() {
            let bucket_id = alloc.bucketid.unwrap() as usize;
            let atlas = &atlases[bucket_id - 1];

            let rect = Rectangle::new(
                Point2D::new(alloc.originx.unwrap(), alloc.originy.unwrap()),
                Point2D::new(alloc.originx.unwrap() + alloc.width, alloc.originy.unwrap() + alloc.height),
            );

            let texture = textures.get_mut(&(alloc.width as _, alloc.height as _))
                .unwrap()
                .pop_front()
                .unwrap();

            lock.texture_mappings.insert(texture as *const _, (bucket_id - 1, Allocation {
                rectangle: rect,
                id: unsafe{ transmute(0) },
            }));

            cast_texture_to_atlas(&device, &queue, &cast_render_pipeline, texture, atlas, &rect);
        }

        lock.shelf_allocators = Vec::new();
        lock.textures = atlases;
    }

    fn push_new_bin(bins: &mut Vec<Bucket>, i: usize) {
        bins.push(Bucket::new(
            MAX_TEXTURE_SIZE as _,
            MAX_TEXTURE_SIZE as _,
            0, 0,
            i as i32 + 1
        ))
    }
}

fn cast_texture_to_atlas(device: &Device, queue: &Queue, cast_render_pipeline: &wgpu::RenderPipeline, src: &TextureInner, dest: &Texture, scissor: &Rectangle) {
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut r_pass =
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dest.view(),
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                    resolve_target: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
        r_pass.set_pipeline(&cast_render_pipeline);
        r_pass.set_bind_group(0, Some(&src.bind_group), &[]);
        let x = scissor.min.x as u32;
        let y = scissor.min.y as u32;
        let w = scissor.max.x as u32 - x;
        let h = scissor.max.y as u32 - y;
        r_pass.set_scissor_rect(x, y, w, h);
        r_pass.set_viewport(
            x as _, y as _,
            w as _, h as _,
            0.0, 1.0
        );
        r_pass.draw(0..3, 0..1);
    }
    let _ = queue.submit(Some(encoder.finish()));
    //device.poll(PollType::WaitForSubmissionIndex(i)).unwrap();
}

#[cfg(test)]
mod tests {
    use std::{fs, io};
    use std::collections::HashMap;
    use std::fs::DirEntry;
    use std::path::Path;
    use std::sync::{LazyLock, Mutex};
    use std::sync::mpsc::channel;
    use image::{ColorType, ExtendedColorType};
    use rand::Rng;
    use wgpu::{Device, MapMode, Queue, TextureFormat};
    use wgpu::wgt::PollType;
    use crate::Renderer;
    use crate::texture::Texture;

    #[test]
    fn group_random_textures() {
        let mut renderer = Renderer::new(512, 512, 1).unwrap();


        let spritesheet = crate::texture::Spritesheet::new(renderer.clone());

        for _ in 0..16 {
            let mut images = Vec::with_capacity(16);
            while images.len() < images.capacity() {
                let image = create_random_image(&mut renderer);
                if let Some(image) = image {
                    images.push(image);
                }
            }
            spritesheet.add_texture(&images);
        }

        let lock = renderer.0.lock().unwrap();

        let device = lock.device.clone();
        let queue = lock.queue.clone();
        device.poll(PollType::Wait).unwrap();


        output_atlases(&device, &queue, spritesheet.inner.lock().unwrap().textures.as_slice());
    }

    static RANDOM_TEXTURES: LazyLock<Mutex<HashMap<String, Texture>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    fn create_random_image(renderer: &mut Renderer) -> Option<Texture> {
        let directory = fs::read_dir("artwork").unwrap();
        let mut rng = rand::rng();

        let files: Vec<io::Result<DirEntry>> = directory.collect();
        let r = rng.random_range(0..files.len());

        let entry = &files[r];
        if entry.is_ok() {
            let path_str = String::from(entry.as_ref().unwrap().path().to_str().unwrap());
            let path = Path::new(&path_str);

            let mut lock = RANDOM_TEXTURES.lock().unwrap();
            return match lock.get(&path_str) {
                Some(texture) => Some(texture.clone()),
                None => {
                    let image = renderer.load_texture_from_path(path).unwrap();
                    lock.insert(path_str, image.clone());
                    Some(image)
                }
            }
        }

        None
    }

    fn output_atlases(device: &Device, queue: &Queue, textures: &[Texture]) {
        // Create a folder if needed
        fs::create_dir_all("./atlas/").unwrap();
        let mut receivers = Vec::new();
        for (i, atlas) in textures.iter().enumerate() {
            let texture = atlas.clone();

            let buffer = texture.write_to_new_buffer(&device, &queue);
            let buffer_cloned = buffer.clone();

            let color_type = format_to_color_type(texture.format());

            let width = texture.width() as u64;
            let height = texture.height() as u64;
            let bytes_per_pixel = color_type.bytes_per_pixel() as u64;
            let size = width * height * bytes_per_pixel;

            let (sender, receiver) = channel();
            buffer.map_async(MapMode::Read, 0..size, move |r| {
                if r.is_ok() {
                    let path = format!("atlas/{}.png", i);
                    let path = Path::new(&path);
                    let data = buffer_cloned.get_mapped_range(0..size);
                    image::save_buffer(path, data.iter().as_ref(), texture.width(), texture.height(), ExtendedColorType::from(color_type)).unwrap();
                }

                sender.send(r).unwrap();
            });
            receivers.push((receiver, buffer));
        }

        device.poll(PollType::Wait).unwrap();
        for (r, buffer) in receivers {
            r.recv().unwrap().unwrap();
            buffer.unmap();
        }
    }

    fn format_to_color_type(format: TextureFormat) -> ColorType {
        match format {
            TextureFormat::R8Unorm => ColorType::L8,
            TextureFormat::Rg8Unorm => ColorType::Rgb8,
            TextureFormat::R16Unorm => ColorType::L16,
            TextureFormat::Rgba8Unorm => ColorType::Rgba8,
            TextureFormat::Rg16Unorm => ColorType::Rgb16,
            TextureFormat::Rgba16Unorm => ColorType::Rgba16,
            TextureFormat::Rgba32Float => ColorType::Rgba32F,

            _ => unimplemented!(),
        }
    }
}