pub trait RenderTarget {
    fn color_target_states(&self) -> Vec<Option<wgpu::ColorTargetState>>;
}