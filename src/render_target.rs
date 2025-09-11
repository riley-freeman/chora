pub trait RenderTarget {
    fn color_target_states(&self) -> Vec<Option<wgpu::ColorTargetState>>;
    fn depth_stencil_state(&self) -> Option<wgpu::DepthStencilState>;
}