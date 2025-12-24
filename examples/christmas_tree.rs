use cgmath::{Euler, Quaternion, Rad, Vector3};
use chora::{Renderer, coordination::Transform};
use chora::render_pipeline::RenderPipelineFlags;
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};
use wgpu::{AddressMode, FilterMode};
use std::path::Path;

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new()
        .with_title("Chora Christmas Tree")
        .build(&event_loop)
        .unwrap();

    let size = window.inner_size();
    let mut renderer = Renderer::new(size.width, size.height, 2).unwrap();

    // Create surface
    renderer.create_surface(&window).unwrap();

    // Load the GLTF model
    let (gltf, buffers, _images) = gltf::import("models/tree/tree.glb").expect("Failed to load GLTF");

    // Load texture
    let texture = renderer.load_texture_from_path(Path::new("models/tree/textures/lambert1_baseColor.jpeg")).unwrap();
    let textures = vec![texture];
    let sampler = renderer.create_sampler(AddressMode::Repeat, FilterMode::Linear);

    // Create shader for textured model
    let shader_source = r#"
    
        @group(1) @binding(0) var tree_texture: texture_2d<f32>;
        @group(1) @binding(1) var tree_sampler: sampler;

        struct VertexInput {
            @location(0) position: vec3<f32>,
            @location(1) tex_coord: vec2<f32>,
            //@location(2) color: vec3<f32>,
        };

        struct VertexOutput {
            @builtin(position) clip_position: vec4<f32>,
            @location(0) tex_coords: vec2<f32>,
        };

        @vertex
        fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
            var out: VertexOutput;
            out.clip_position = camera.view * instance.model * vec4<f32>(vertex.position, 1.0);
            out.tex_coords = vertex.tex_coord.xy;
            return out;
        }

        @fragment
        fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
            return textureSample(tree_texture, tree_sampler, in.tex_coords);
        }
    "#;

    let render_pipeline = renderer.create_render_pipeline(
        shader_source,
        &textures,
        Some(sampler),
        RenderPipelineFlags::all() & !RenderPipelineFlags::ALLOW_WORLD_UNIFORM,
    );

    // Extract vertices and indices from GLTF
    let mut all_vertices = Vec::new();
    let mut all_indices: Vec<i32> = Vec::new();

    for mesh in gltf.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            // Read positions and tex coords together
            let positions: Vec<[f32; 3]> = reader.read_positions().map(|iter| iter.collect()).unwrap_or_default();
            let tex_coords: Vec<[f32; 2]> = reader.read_tex_coords(0)
                .map(|iter| iter.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

            // Interleave positions and tex coords
            for (position, tex_coord) in positions.iter().zip(tex_coords.iter()) {
                // Position (x, y, z)
                all_vertices.push(position[0]);
                all_vertices.push(position[1]);
                all_vertices.push(position[2]);
                // Tex coord (u, v)
                all_vertices.push(tex_coord[0]);
                all_vertices.push(tex_coord[1]);
            }

            // Read indices
            if let Some(indices) = reader.read_indices() {
                let base_index = (all_indices.len() / 3) as i32;
                for index in indices.into_u32() {
                    all_indices.push(base_index + index as i32);
                }
            }
        }
    }

    println!("Loaded tree with {} vertices and {} indices", all_vertices.len() / 5, all_indices.len());

    let mesh = renderer.create_mesh(&all_vertices, &all_indices, render_pipeline).unwrap();

    let mut transform = Box::new(Transform::default());
    transform.set_position(Vector3::new(0.0, -2.0, -5.0));
    let model = renderer.create_model(vec![mesh], true, &transform).unwrap();

    renderer.add_to_render_queue(&model).unwrap();

    let mut rotation = 0.0f32;
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
                // Rotate on Y axis
                rotation += 0.01;
                transform.set_rotation(Quaternion::from(Euler::new(Rad(0.0), Rad(rotation), Rad(0.0))));

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
