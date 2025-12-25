use cgmath::{EuclideanSpace, Euler, Quaternion, Rad, Vector3, Zero};
use chora::{Renderer, coordination::Transform};
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
        .with_title("Chora Cube Test")
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
    @location(2) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view * instance.model * vec4<f32>(vertex.position, 1.0);
    out.color = vertex.color;
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
        RenderPipelineFlags::all() & ! RenderPipelineFlags::ALLOW_WORLD_UNIFORM,
    );

    // Create cube mesh
    // 8 vertices for a cube, each with position (x, y, z) and color (r, g, b)
    #[rustfmt::skip]
    let vertices = [
        // Front face (red-ish)
        -0.5, -0.5,  0.5,  1.0, 0.0, 0.0,  // 0: bottom-left-front
         0.5, -0.5,  0.5,  1.0, 0.2, 0.0,  // 1: bottom-right-front
         0.5,  0.5,  0.5,  1.0, 0.4, 0.0,  // 2: top-right-front
        -0.5,  0.5,  0.5,  1.0, 0.6, 0.0,  // 3: top-left-front

        // Back face (blue-ish)
        -0.5, -0.5, -0.5,  0.0, 0.0, 1.0,  // 4: bottom-left-back
         0.5, -0.5, -0.5,  0.0, 0.2, 1.0,  // 5: bottom-right-back
         0.5,  0.5, -0.5,  0.0, 0.4, 1.0,  // 6: top-right-back
        -0.5,  0.5, -0.5,  0.0, 0.6, 1.0,  // 7: top-left-back
    ];

    // 12 triangles (2 per face, 6 faces)
    #[rustfmt::skip]
    let indices = [
        // Front face
        0, 1, 2,  0, 2, 3,
        // Right face
        1, 5, 6,  1, 6, 2,
        // Back face
        5, 4, 7,  5, 7, 6,
        // Left face
        4, 0, 3,  4, 3, 7,
        // Top face
        3, 2, 6,  3, 6, 7,
        // Bottom face
        4, 5, 1,  4, 1, 0,
    ];

    let mesh = renderer.create_mesh(&vertices, &indices, render_pipeline).unwrap();

    let mut transform = Box::new(Transform::default());
    transform.set_position(Vector3::new(0.0, 0.0, -2.0));
    let model = renderer.create_model(vec![mesh], true, &transform).unwrap();

    renderer.add_to_render_queue(&model).unwrap();

    let mut scalar = 0.0f32;
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
                scalar += 0.015;
                transform.set_position_y(f32::sin(scalar) * 0.2);
                transform.set_rotation(Quaternion::from(Euler::new(Rad(3.14/4.0), Rad(scalar), Rad(scalar))));
                
                println!("RENDER: {:?}", transform.rotation());
                renderer.render().unwrap();
                renderer.present().unwrap();
                window.request_redraw();
            }
            Event::AboutToWait => {
                window.request_redraw();
            }
            _ => {}
        }
    }).unwrap();
}
