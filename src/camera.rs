use std::sync::{Arc, Mutex};
use std::default::Default;
use std::mem::size_of;

use wgpu::wgt::BufferDescriptor;
use wgpu::{BufferUsages, ColorTargetState, ColorWrites};
use wgpu::TextureViewDescriptor;
use wgpu::Buffer;
use wgpu::CompareFunction::Less;
use wgpu::Device;
use wgpu::Texture;
use wgpu::TextureDescriptor;
use wgpu::TextureFormat;
use wgpu::TextureUsages;
use wgpu::TextureView;
use crate::error::ChoraError;
use crate::render_target::RenderTarget;

#[allow(unused)]
struct CameraInner {
    fov: f32,
    orthographic: bool,
    hdr: bool,

    // Positioning and Rotation pointers
    position: *const [f32; 3],
    pitch: *const f32,
    yaw: *const f32,
    roll: *const f32,

    output_images: Vec<Texture>,
    depth_image: Texture,

    output_image_views: Vec<TextureView>,
    depth_image_view: TextureView,

    camera_buffers: Vec<Buffer>,
}

#[derive(Clone)]
pub struct Camera(Arc<Mutex<CameraInner>>);

#[allow(unused)]
struct CameraBufferStruct {
    view_matrix: cgmath::Matrix4<f32>,
    proj_matrix: cgmath::Matrix4<f32>,
}

impl Camera {
    pub fn new(
        device: &Device,
        width: u32,
        height: u32,
        buffers: usize,
        hdr: bool,
        fov: f32,
        orthographic: bool,
        position: &[f32; 3],
        pitch: &f32,
        yaw: &f32,
        roll: &f32,
    ) -> Result<Self, ChoraError> {
        // Create an output / resolve texture (when I implement MSAA)
        let mut output_images = Vec::with_capacity(buffers);
        let mut output_image_views = Vec::new();
        for _ in 0..buffers {
            let (output_image, output_view) = create_camera_texture(
                device, 
                width,
                height,
                hdr, 
                MSAASampleCount::X1
            );

            output_images.push(output_image);
            output_image_views.push(output_view);
        }


        // Create a depth image
        let (depth_image, depth_image_view) = create_depth_texture(device, width, height);

        // Create a camera buffer... for View Projection Matrices
        let mut camera_buffers = Vec::new();
        for _ in 0..buffers {
            let buffer = device.create_buffer(&BufferDescriptor {
                size: size_of::<CameraBufferStruct>() as u64,
                mapped_at_creation: false,
                usage: BufferUsages::UNIFORM,

                label: None,
            });
            camera_buffers.push(buffer);
        }

        let inner = CameraInner {
            fov, 
            orthographic,
            hdr,
            position: position as _,
            pitch: pitch as _,
            yaw: yaw as _, 
            roll: roll as _, 
            output_images, 
            depth_image,
            output_image_views,
            depth_image_view,
            camera_buffers,
        };

        Ok(Self(Arc::new(Mutex::new(inner))))
    }

    pub fn width(&self) -> u32 {
        let lock = self.0.lock().unwrap();
        lock.output_images[0].width()
    }

    pub fn height(&self) -> u32 {
        let lock = self.0.lock().unwrap();
        lock.output_images[0].height()
    }

    pub(crate) fn current_output_texture_view(&self) -> TextureView {
        let lock = self.0.lock().unwrap();
        lock.output_image_views[0].clone()
    }

    pub(crate) fn current_output_texture_raw(&self) -> Texture {
        let lock = self.0.lock().unwrap();
        lock.output_images[0].clone()
    }
}

unsafe impl Sync for Camera {}
unsafe impl Send for Camera {}

impl RenderTarget for Camera {
    fn color_target_states(&self) -> Vec<Option<ColorTargetState>> {
        let lock = self.0.lock().unwrap();

        // Get the format
        let format = if lock.hdr {
            TextureFormat::Rgba16Float
        } else {
            TextureFormat::Rgba8Unorm
        };

        let solo = Some(ColorTargetState {
            format,
            write_mask: ColorWrites::ALL,
            blend: None,
        });

        vec![solo]
    }

    fn depth_stencil_state(&self) -> Option<wgpu::DepthStencilState> {
        Some(wgpu::DepthStencilState {
            format: find_camera_depth_format(),
            depth_write_enabled: true,
            depth_compare: Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        })
    }
}

fn create_camera_texture(
    device: &Device,
    width: u32,
    height: u32,
    hdr: bool,
    samples: MSAASampleCount,
) -> (Texture, TextureView) {
    let format = find_camera_format(hdr);
    let desc = TextureDescriptor {
        dimension: wgpu::TextureDimension::D2,
        format,
        mip_level_count: 1,
        sample_count: samples.into(),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],

        label: None,
    };
    let texture = device.create_texture(&desc);

    let view_desc = texture_view_desc(format);
    let view = texture.create_view(&view_desc);

    (texture, view)
}

fn create_depth_texture(
    device: &Device,
    width: u32,
    height: u32,
) -> (Texture, TextureView) {
    let format = find_camera_depth_format();
    let desc = TextureDescriptor {
        dimension: wgpu::TextureDimension::D2,
        format,
        mip_level_count: 1,
        sample_count: 1,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],

        label: None,
    };
    let texture = device.create_texture(&desc);

    let view_desc = texture_view_desc(format);
    let view = texture.create_view(&view_desc);


    (texture, view)
}

fn texture_view_desc<'a>(format: TextureFormat) -> TextureViewDescriptor<'a> {
    TextureViewDescriptor {
        aspect: wgpu::TextureAspect::All,
        array_layer_count: None,
        mip_level_count: None,
        base_array_layer: 0,
        base_mip_level: 0,
        dimension: Some(wgpu::TextureViewDimension::D2),
        format: Some(format),
        usage: Some(TextureUsages::RENDER_ATTACHMENT),

        label: None,
    }
}

fn find_camera_format(hdr: bool) -> TextureFormat {
    match hdr {
        false => TextureFormat::Rgba8Unorm,
        true => TextureFormat::Rgba16Float,
    }
}

fn find_camera_depth_format() -> TextureFormat {
    TextureFormat::Depth24PlusStencil8
}

#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum MSAASampleCount {
    #[default]
    X1,
    X2,
    X4,
}

impl Into<u32> for MSAASampleCount {
    fn into(self) -> u32 {
        match self {
            MSAASampleCount::X1 => 1,
            MSAASampleCount::X2 => 2,
            MSAASampleCount::X4 => 4,
        }
    }
}

#[cfg(test)]
mod tests {}



