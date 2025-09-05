use std::collections::hash_map::HashMap;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::default::Default;
use std::mem::size_of;

use wgpu::wgt::BufferDescriptor;
use wgpu::BufferUsages;
use wgpu::TextureViewDescriptor;
use wgpu::Buffer;
use wgpu::Device;
use wgpu::Texture;
use wgpu::TextureDescriptor;
use wgpu::TextureFormat;
use wgpu::TextureUsages;
use wgpu::TextureView;

use crate::error::ChoraError;
use crate::mesh::{Mesh, WeakMesh};
use crate::model::Model;

use crate::linked_list::LinkedList;

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


    // Mesh Database (I think I don't really know what this is called)
    mesh_collection: HashMap<*const c_void, LinkedList<WeakMesh>>,

    independent_renders: HashMap<*const c_void, ()>,
    instanced_renders: HashMap<*const c_void, usize>,
}

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

            mesh_collection: Default::default(),

            independent_renders: Default::default(),
            instanced_renders: Default::default(),
        };

        Ok(Self(Arc::new(Mutex::new(inner))))
    }

    pub fn add_model(&mut self, model: &Model) -> Result<(), ChoraError> {
        for mesh in model.into_iter() {
            self.add_mesh(mesh)?;
        }
        Ok(())
    }

    fn add_mesh(&mut self, mesh: &Mesh) -> Result<(), ChoraError> {
        let mesh_address = mesh.0.as_ref() as *const _ as *const c_void;
        let weak_mesh = WeakMesh(Arc::downgrade(&mesh.0));
        let mut this = self.0.lock().unwrap();


        // Organize the mesh into groups
        let mesh_collection= this.mesh_collection
            .entry(mesh_address)
            .or_insert(LinkedList::new());

        let _mesh_collection_node = mesh_collection
            .push_front(weak_mesh.clone());

        // Check for instanceable meshes
        let mesh_collection = this.mesh_collection
            .get(&(mesh.0.as_ref() as *const _ as _)).unwrap();
        let mesh_collection_len = mesh_collection.len();

        if mesh_collection_len > 1 {
            if mesh_collection_len == 2 {
                this.independent_renders.remove(&mesh_address);
                this.instanced_renders.insert(mesh_address, 1);
            }
            *this.instanced_renders.get_mut(&mesh_address).unwrap() += 1;
        } else {
            // Create a single independent render.
            this.independent_renders.insert(mesh_address, ());
        }

        Ok(())
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
    let format = TextureFormat::Depth24PlusStencil8;
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
mod tests {
    use crate::Chora;
    use super::*;

    use std::mem::drop;
    /// Tests the mesh grouping functionality of the camera
    ///
    /// This test verifies that:
    /// 1. Single meshes are added as independent renders
    /// 2. Multiple identical meshes are grouped into instanced renders
    /// 3. Different meshes remain as separate render groups
    /// 4. Mesh collection tracking works correctly
    #[test]
    pub fn independent_instanced_grouping_test() {
        let renderer = Chora::new(2).unwrap();

        // Camera setup
        let pos = cgmath::vec3(0.0f32, 0.0f32, 0.0f32);
        let pitch = 0.0f32;
        let yaw = 0.0f32;
        let roll = 0.0f32;
        let mut camera = renderer.create_camera(
            512,
            512,
            true,
            77.0,
            false,
            pos.as_ref(),
            &pitch,
            &yaw,
            &roll,
        ).unwrap();

        // Create test meshes
        let vertices = [
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
            [-0.0, -0.5, 0.0],
        ];
        let vertices = vertices.iter().flat_map(|v| v.iter()).copied().collect::<Vec<f32>>();
        let indices = [0, 1, 2];

        let i_triangle0 = renderer.create_mesh(&vertices, &indices).unwrap();
        let i_triangle1 = Mesh(Arc::clone(&i_triangle0.0));
        let s_triangle2 = renderer.create_mesh(&vertices, &indices).unwrap();

        // Test single mesh (should be independent)
        camera.add_mesh(&i_triangle0).unwrap();
        {
            let lock = camera.0.lock().unwrap();
            assert_eq!(lock.independent_renders.len(), 1, "Single mesh should be independent");
            assert_eq!(lock.instanced_renders.len(), 0, "No instanced renders should exist");
            assert_eq!(lock.mesh_collection.len(), 1, "Should have one mesh collection");
            assert_eq!(lock.mesh_collection.values().next().unwrap().len(), 1, "Collection should have one mesh");
        }

        // Test identical mesh (should become instanced)
        camera.add_mesh(&i_triangle1).unwrap();
        {
            let lock = camera.0.lock().unwrap();
            assert_eq!(lock.independent_renders.len(), 0, "No independent renders should remain");
            assert_eq!(lock.instanced_renders.len(), 1, "Should have one instanced render");
            assert_eq!(lock.mesh_collection.len(), 1, "Should have one mesh collection");
            assert_eq!(lock.mesh_collection.values().next().unwrap().len(), 2, "Collection should have two meshes");

            let i_render = lock.instanced_renders.iter().nth(0).unwrap();
            assert_eq!(*i_render.1, 2, "Instance count should be 2");
        }

        // Test different mesh (should be independent)
        camera.add_mesh(&s_triangle2).unwrap();
        {
            let lock = camera.0.lock().unwrap();
            assert_eq!(lock.independent_renders.len(), 1, "Should have one independent render");
            assert_eq!(lock.instanced_renders.len(), 1, "Should have one instanced render");
            assert_eq!(lock.mesh_collection.len(), 2, "Should have two mesh collections");

            let i_render = lock.instanced_renders.iter().nth(0).unwrap();
            assert_eq!(*i_render.1, 2, "Instance count should remain 2");
        }

        // Clean up
        drop(i_triangle0);
        drop(i_triangle1);
        drop(s_triangle2);
        drop(camera);
    }
}



