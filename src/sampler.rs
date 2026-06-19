use crate::{Renderer, RendererInner};
use std::sync::{Arc, Mutex};
use wgpu::{AddressMode, FilterMode};

pub(crate) struct SamplerInner {
    pub(crate) sampler: wgpu::Sampler,
}

#[derive(Clone)]
pub struct Sampler {
    pub(crate) inner: Arc<Mutex<SamplerInner>>,
}

impl Sampler {
    pub(crate) fn new_locked(
        renderer: Renderer,
        r_inner: &RendererInner,
        address_mode: AddressMode,
        filter_mode: FilterMode,
    ) -> Self {
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
        let sampler = r_inner.device.create_sampler(&desc);

        let inner = SamplerInner { sampler };

        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }
}
