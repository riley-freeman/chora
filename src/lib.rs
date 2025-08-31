use std::sync::LazyLock;

use wgpu::wgt::DeviceDescriptor;
use wgpu::BackendOptions;
use wgpu::Backends;
use wgpu::Instance;
use wgpu::InstanceDescriptor;
use wgpu::InstanceFlags;
use wgpu::MemoryBudgetThresholds;
use wgpu::RequestAdapterOptions;

use wgpu::Adapter;
use wgpu::Device;
use wgpu::Queue;

use crate::camera::Camera;

pub mod error;
pub mod camera;


static INSTANCE :LazyLock<Instance> = LazyLock::new(|| {
    Instance::new(&InstanceDescriptor {
        backends: Backends::PRIMARY,
        #[cfg(debug_assertions)]
        flags: InstanceFlags::DEBUG,
        #[cfg(not(debug_assertions))]
        flags: InstanceFlags::empty(),
        memory_budget_thresholds: MemoryBudgetThresholds {
            for_resource_creation: None,
            for_device_loss: None
        },
        backend_options: BackendOptions {..Default::default()}
    })
});

#[allow(unused)]
pub struct Chora {
    adapter: Adapter,
    device: Device,
    queue: Queue,
    buffers: usize,
}

impl Chora {
    pub fn new(buffers: usize) -> Result<Self, error::ChoraError> {
        let adapter = pollster::block_on(INSTANCE.request_adapter(&RequestAdapterOptions {
            ..Default::default()
        })).map_err(|_| error::ChoraError::FailedToFindAdapter {})?;

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: Some("0x99 CRAYON CHORA"),
            ..Default::default()
        })).map_err(|_| error::ChoraError::FailedGettingSuitableDevice {})?;

        Ok(Chora { 
            adapter,
            device,
            queue,
            buffers
        })
    }

    pub fn create_camera(
        &self,
        width: u32,
        height: u32,
        hdr: bool,
        fov: f32,
        orthographic: bool,
        position: &[f32; 3],
        pitch: &f32,
        yaw: &f32,
        roll: &f32,
    ) -> Result<Camera, error::ChoraError> {
        Camera::new(
            &self.device,
            width, height,
            self.buffers,
            hdr, fov, orthographic,
            position,
            pitch, yaw, roll,
        )
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_world() {
        println!("Hello, World!");
    }

    #[test]
    fn new_renderer() {
        let renderer = Chora::new(2).unwrap();

        let pos = cgmath::vec3(0.0f32, 0.0f32, 0.0f32);
        let pitch = 0.0f32;
        let yaw = 0.0f32;
        let roll = 0.0f32;
        renderer.create_camera(512, 512, true, 77.0, false, pos.as_ref(), &pitch, &yaw, &roll).unwrap();
    }
}
