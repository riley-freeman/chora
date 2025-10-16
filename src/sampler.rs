use std::sync::{Arc, Mutex};
use wgpu::{AddressMode, Device, FilterMode};
use crate::Renderer;

pub(crate) struct SamplerInner {
    pub(crate) sampler: wgpu::Sampler,
    _renderer: Renderer,
}

#[derive(Clone)]
pub struct Sampler {
    pub(crate) inner: Arc<Mutex<SamplerInner>>,
}

impl Sampler {
    pub fn new(renderer: Renderer, device: &Device, address_mode: AddressMode, filter_mode: FilterMode) -> Self {
        let desc = wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: address_mode,
            address_mode_v: address_mode,
            address_mode_w: address_mode,
            mag_filter: filter_mode,
            min_filter: filter_mode,
            mipmap_filter: filter_mode,
            ..Default::default()
        };

        let sampler = device.create_sampler(&desc);

        let inner = SamplerInner {
            sampler,
            _renderer: renderer,
        };

        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }
}
