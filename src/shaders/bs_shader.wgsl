// Vertex shader
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

struct WorldUniform {
}

struct CameraUniform {
    view_proj: mat4x4<f32>,
}


@group(0) @binding(0)
var<uniform> world: WorldUniform;

@group(0) @binding(1)
var<uniform> camera: CameraUniform;


@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Define the three vertices of our triangle in clip space
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 0.5),    // Top-center
        vec2<f32>(-0.5, -0.5),  // Bottom-left
        vec2<f32>(0.5, -0.5)    // Bottom-right
    );

    var output: VertexOutput;
    output.position = vec4<f32>(pos[vertex_index], 0.0, 1.0);
    output.color = vec4<f32>(1.0, 1.0, 1.0, 1.0);  // White color

    return output;
}

// Fragment shader
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;  // Output the white color
}