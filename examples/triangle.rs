use chora::Renderer;
use chora::render_pipeline::RenderPipelineFlags;
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};
use wgpu::{AddressMode, FilterMode};

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new()
        .with_title("Chora Triangle Test")
        .build(&event_loop)
        .unwrap();

    let size = window.inner_size();
    let mut renderer = Renderer::new(size.width, size.height, 2).unwrap();

    // Create surface
    renderer.create_surface(&window).unwrap();

    // Create a simple shader
    let shader_source = r#"
        struct VertexInput {
            @location(0) position: vec3<f32>,
            // @location(1) tex_coords: vec2<f32>,
            @location(2) color: vec3<f32>,
        };

        struct VertexOutput {
            @builtin(position) clip_position: vec4<f32>,
            @location(0) color: vec3<f32>,
        };

        @vertex
        fn vs_main(in: VertexInput) -> VertexOutput {
            var out: VertexOutput;
            out.clip_position = vec4<f32>(in.position, 1.0);
            out.color = in.color;
            return out;
        }

        @fragment
        fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
            return vec4<f32>(in.color, 1.0);
        }
    "#;

    // Create a dummy texture (required by pipeline creation for now)
    let texture = renderer.create_texture(
        1, 1, 
        wgpu::TextureFormat::Rgba8Unorm, 
        Some(&[255, 255, 255, 255])
    );
    let textures = vec![texture];
    let sampler = renderer.create_sampler(AddressMode::ClampToEdge, FilterMode::Nearest);

    let render_pipeline = renderer.create_render_pipeline(
        shader_source,
        &textures,
        Some(sampler),
        RenderPipelineFlags::empty(),
    );

    // Create triangle mesh
    let vertices = [
        -0.5,  0.5, 0.0, 0.0, 1.0, 0.0,
         0.5,  0.5, 0.0, 0.0, 0.0, 1.0,
         0.0, -0.5, 0.0, 1.0, 0.0, 0.0,
    ];
    let indices = [0, 1, 2];

    let mesh = renderer.create_mesh(&vertices, &indices, render_pipeline).unwrap();
    renderer.add_mesh_to_render_queue(&mesh).unwrap();

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                elwt.exit();
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(physical_size),
                ..
            } => {
                renderer.resize_surface(physical_size.width, physical_size.height).unwrap();
                window.request_redraw();
            }
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                renderer.render().unwrap();
                renderer.present().unwrap();
            }
            _ => {}
        }
    }).unwrap();
}
