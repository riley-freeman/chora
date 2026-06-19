// Demonstrates automatic instanced rendering with per-instance model matrices.
//
// A single triangle mesh is cloned into N handles sharing the same GPU buffers.
// Each clone is wrapped in a Model with a unique X-offset Transform, so the
// renderer uploads N model matrices into the instance buffer and issues one
// draw_indexed call with instance_count = N.
//
// The vertex shader receives the model matrix via InstanceInput and applies it
// to position each triangle; @builtin(instance_index) is used only for hue.

use chora::coordination::Transform;
use chora::model::Model;
use chora::render_pipeline::RenderPipelineFlags;
use chora::Renderer;
use cgmath::{Quaternion, Rad, Rotation3, Vector3};
use std::f32::consts::TAU;
use std::time::Instant;
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};
use wgpu::{AddressMode, FilterMode};

const INSTANCE_COUNT: usize = 7;

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new()
        .with_title("Chora — Instanced Rendering via InstanceInput")
        .build(&event_loop)
        .unwrap();

    let size = window.inner_size();
    let mut renderer = Renderer::new(size.width, size.height, 2).unwrap();
    renderer.create_surface(&window).unwrap();

    // OVERRIDE_VERTEX_INPUT keeps the custom VertexInput struct (20-byte stride).
    // ALLOW_OBJECT_UNIFORM injects RawInstanceInput/InstanceInput and adds the
    // per-instance vertex buffer layout (model matrix at locations 4–7).
    let shader_source = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(
    vertex: VertexInput,
    instance: InstanceInput,
    @builtin(instance_index) idx: u32,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = instance.model * vec4<f32>(vertex.position, 1.0);

    // Cycle hue evenly across the visible spectrum.
    let n = f32(7);
    let t = f32(idx) / n * 6.28318;
    out.color = vec3<f32>(
        0.5 + 0.5 * cos(t),
        0.5 + 0.5 * cos(t + 2.09439),
        0.5 + 0.5 * cos(t + 4.18879),
    );

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
"#;

    let texture = renderer.create_texture(
        1, 1,
        wgpu::TextureFormat::Rgba8Unorm,
        Some(&[255, 255, 255, 255]),
    );
    let sampler = renderer.create_sampler(AddressMode::ClampToEdge, FilterMode::Nearest);

    let pipeline = renderer.create_render_pipeline(
        shader_source,
        &vec![texture],
        Some(sampler),
        RenderPipelineFlags::OVERRIDE_VERTEX_INPUT | RenderPipelineFlags::ALLOW_OBJECT_UNIFORM,
    );

    // Triangle vertices: [x, y, z, u, v]  (stride = 20 bytes)
    #[rustfmt::skip]
    let vertices: [f32; 15] = [
         0.0,  0.12, 0.0,  0.5, 0.0,   // top
        -0.09, -0.09, 0.0, 0.0, 1.0,   // bottom-left
         0.09, -0.09, 0.0, 1.0, 1.0,   // bottom-right
    ];
    let indices: [i32; 3] = [0, 1, 2];

    let base_mesh = renderer.create_mesh(&vertices, &indices, pipeline).unwrap();

    let n = INSTANCE_COUNT as f32;
    let spacing = 0.28;
    let identity_rot = Quaternion::new(1.0, 0.0, 0.0, 0.0);

    // Box<Transform> keeps each transform at a stable heap address; Model stores
    // a raw pointer to the transform, so it must not move after creation.
    let mut transforms: Vec<Box<Transform>> = Vec::with_capacity(INSTANCE_COUNT);
    let mut models: Vec<Model> = Vec::with_capacity(INSTANCE_COUNT);

    for i in 0..INSTANCE_COUNT {
        let offset_x = (i as f32 - (n - 1.0) * 0.5) * spacing;
        let transform = Box::new(Transform::new(
            Vector3::new(offset_x, 0.0, 0.5),
            identity_rot,
            Vector3::new(1.0, 1.0, 1.0),
        ));
        let mesh = base_mesh.clone();
        let model = renderer.create_model(vec![mesh], false, &*transform).unwrap();
        transforms.push(transform);
        models.push(model);
    }

    for model in &models {
        renderer.add_to_render_queue(&model).unwrap();
    }

    let start = Instant::now();
    let phase_step = TAU / INSTANCE_COUNT as f32;

    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => elwt.exit(),
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => renderer.resize_surface(size.width, size.height).unwrap(),
            Event::AboutToWait => window.request_redraw(),
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                let t = start.elapsed().as_secs_f32();
                for (i, transform) in transforms.iter_mut().enumerate() {
                    let phase = i as f32 * phase_step;
                    let offset_x = (i as f32 - (n - 1.0) * 0.5) * spacing;
                    transform.set_position(Vector3::new(offset_x, (t + phase).sin() * 0.15, 0.5));
                    transform.set_rotation(Quaternion::from_axis_angle(
                        Vector3::new(0.0, 1.0, 0.0),
                        Rad(t * 1.5 + phase),
                    ));
                }
                renderer.render().unwrap();
                renderer.present().unwrap();
            }
            _ => {}
        }
        // Keep transforms and models alive for the duration of the event loop.
        let _ = (&transforms, &models);
    }).unwrap();
}
