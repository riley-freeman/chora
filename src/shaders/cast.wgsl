struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) texture_coords: vec2<f32>,
}

@group(0) @binding(0) var texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;


@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let x = f32(vertex_index & 1u) * 4.0 - 1.0;  // 0->-1, 1->3, 2->-1
    let y = f32(vertex_index & 2u) * 2.0 - 1.0;  // 0->-1, 1->-1, 2->3
    
    var output: VertexOutput;
    output.position = vec4<f32>(x, y, 0.0, 1.0);
    
    let uv_x = (x + 1.0) * 0.25;  // Map [-1,3] to [0,1]
    let uv_y = (y + 1.0) * 0.25;  // Map [-1,3] to [0,1]
    
    output.texture_coords = vec2<f32>(uv_x, uv_y);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(texture, texture_sampler, input.texture_coords);
}


