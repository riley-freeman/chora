use std::ptr::slice_from_raw_parts;
use std::sync::{Arc, Mutex};
use std::default::Default;
use std::mem::{offset_of, size_of};

use cgmath::{Matrix4, Vector3, Rad, Quaternion, Rotation3, PerspectiveFov, Ortho};
use wgpu::wgt::BufferDescriptor;
use wgpu::{BindGroup, BindGroupLayout, BufferUsages, ColorTargetState, ColorWrites};
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

    view_proj_matrix: Matrix4<f32>,
    camera_buffers: Vec<Buffer>,
    bind_groups: Vec<BindGroup>,

    output_images: Vec<Texture>,
    depth_image: Texture,

    output_image_views: Vec<TextureView>,
    depth_image_view: TextureView,
}

#[derive(Clone)]
pub struct Camera(Arc<Mutex<CameraInner>>);

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub(crate) struct CameraBufferStruct {
    view_proj_matrix: cgmath::Matrix4<f32>,
}

// Safety: ModelBufferStruct is repr(C) and contains only Matrix4<f32> which is Pod
unsafe impl bytemuck::Pod for CameraBufferStruct {}
unsafe impl bytemuck::Zeroable for CameraBufferStruct {}

impl Camera {
    pub fn new(
        device: &Device,
        bind_group_layout: &BindGroupLayout,
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
        
        // Compute initial view-projection matrix
        let view_proj_matrix = Self::compute_view_proj_matrix(
            position,
            pitch,
            yaw,
            roll,
            fov,
            orthographic,
            width,
            height,
        );

        // Create a camera buffer... for View Projection Matrices
        let mut camera_buffers = Vec::new();
        for _ in 0..buffers {
            let buffer = device.create_buffer(&BufferDescriptor {
                size: size_of::<CameraBufferStruct>() as u64,
                mapped_at_creation: true,
                usage: BufferUsages::UNIFORM,

                label: None,
            });
            
            // Write the view projection matrix to the uniform buffer
            {
                let offset = offset_of!(CameraBufferStruct, view_proj_matrix) as u64;
                let size = size_of::<Matrix4<f32>>() as u64;
                let mut view = buffer.get_mapped_range_mut(offset..size);

                unsafe {
                    let src_ptr = &view_proj_matrix as *const _ as *const u8;
                    let src = &*slice_from_raw_parts(src_ptr, size as usize);
                    view.copy_from_slice(src);
                };
            }
            buffer.unmap();

            camera_buffers.push(buffer);
        }

        // Create bind groups for each camera buffer
        let mut bind_groups = Vec::with_capacity(buffers);
        for buffer in &camera_buffers {
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("camera_bind_group"),
                layout: &bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffer.as_entire_binding(),
                }],
            });
            bind_groups.push(bind_group);
        }



        let inner = CameraInner {
            fov, 
            orthographic,
            hdr,
            position: position as _,
            pitch: pitch as _,
            yaw: yaw as _, 
            roll: roll as _,
            view_proj_matrix,
            camera_buffers,
            bind_groups,
            output_images, 
            depth_image,
            output_image_views,
            depth_image_view,
        };

        Ok(Self(Arc::new(Mutex::new(inner))))
    }

    /// Compute the view-projection matrix from camera parameters
    ///
    /// # Arguments
    /// * `position` - Camera position in world space
    /// * `pitch` - Rotation around X-axis (in radians)
    /// * `yaw` - Rotation around Y-axis (in radians)
    /// * `roll` - Rotation around Z-axis (in radians)
    /// * `fov` - Field of view (in degrees for perspective, or size for orthographic)
    /// * `orthographic` - Whether to use orthographic projection
    /// * `width` - Viewport width in pixels
    /// * `height` - Viewport height in pixels
    ///
    /// # Returns
    /// The combined view-projection matrix
    pub fn compute_view_proj_matrix(
        position: &[f32; 3],
        pitch: &f32,
        yaw: &f32,
        roll: &f32,
        fov: f32,
        orthographic: bool,
        width: u32,
        height: u32,
    ) -> Matrix4<f32> {
        // Compute view matrix
        let view_matrix = Self::compute_view_matrix(position, pitch, yaw, roll);
        
        // Compute projection matrix
        let proj_matrix = Self::compute_projection_matrix(fov, orthographic, width, height);
        
        // Combine: projection * view
        proj_matrix * view_matrix
    }

    /// Compute the view matrix from camera position and rotation
    fn compute_view_matrix(
        position: &[f32; 3],
        pitch: &f32,
        yaw: &f32,
        roll: &f32,
    ) -> Matrix4<f32> {
        // Create rotation quaternions from Euler angles
        // Order: Z (roll) -> X (pitch) -> Y (yaw)
        let rotation_x = Quaternion::from_angle_x(Rad(*pitch));
        let rotation_y = Quaternion::from_angle_y(Rad(*yaw));
        let rotation_z = Quaternion::from_angle_z(Rad(*roll));
        let rotation_quat = rotation_y * rotation_x * rotation_z;
        let rotation_matrix = Matrix4::from(rotation_quat);
        
        // Create translation matrix (negative position because we're moving the world, not the camera)
        let translation = Matrix4::from_translation(Vector3::new(-position[0], -position[1], -position[2]));
        
        // View matrix = rotation * translation
        // The rotation is applied first, then translation
        rotation_matrix * translation
    }

    /// Compute the projection matrix
    fn compute_projection_matrix(
        fov: f32,
        orthographic: bool,
        width: u32,
        height: u32,
    ) -> Matrix4<f32> {
        let aspect = width as f32 / height as f32;
        
        if orthographic {
            // Orthographic projection
            // fov is used as the size/scale of the orthographic view
            let half_size = fov * 0.5;
            let left = -half_size * aspect;
            let right = half_size * aspect;
            let bottom = -half_size;
            let top = half_size;
            let near = 0.1;
            let far = 1000.0;
            
            Matrix4::from(Ortho {
                left,
                right,
                bottom,
                top,
                near,
                far,
            })
        } else {
            // Perspective projection
            let fov_rad = Rad(fov.to_radians());
            let near = 0.1;
            let far = 1000.0;
            
            Matrix4::from(PerspectiveFov {
                fovy: fov_rad,
                aspect,
                near,
                far,
            })
        }
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

    pub(crate) fn depth_texture_view(&self) -> TextureView {
        let lock = self.0.lock().unwrap();
        lock.depth_image_view.clone()
    }

    pub(crate) fn _uniform_buffer(&self, index: usize) -> Option<wgpu::Buffer> {
        let lock = self.0.lock().unwrap();
        lock.camera_buffers.get(index).map(|r| r.clone())
    }

    pub(crate) fn bind_group(&self, index: usize) -> Option<BindGroup> {
        let lock = self.0.lock().unwrap();
        lock.bind_groups.get(index).map(|r| r.clone())
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
        usage: Some(TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING),

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



