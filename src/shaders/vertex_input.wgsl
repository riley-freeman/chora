struct RawVertexInput {
    /// Contains vertex position and the shader ID
    @location(0) pos_id: vec4<f32>,

    /// Contains tangent coords plus the texture U coordinate
    @location(1) tan_u: vec4<f32>,

    /// Contains tangent coords plus the texture U coordinate
    @location(2) bitan_v: vec4<f32>,
}

struct VertexInput {
    shader_id: i32,
    position: vec3<f32>,
    texture: vec2<f32>,
    tangent: vec3<f32>,
    bi_tangent: vec3<f32>,
}

