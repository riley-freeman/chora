struct RawVertexInput {
    /// Contains vertex position and the shader ID
    @location(0) pos: vec4<f32>,

    /// Contains tangent coords plus the texture U coordinate
    @location(1) tan_u: vec4<f32>,

    /// Contains tangent coords plus the texture U coordinate
    @location(2) bitan_v: vec4<f32>,
}

struct VertexInput {
    position: vec3<f32>,
    texture: vec2<f32>,
    tangent: vec3<f32>,
    bi_tangent: vec3<f32>,
}

fn translate_raw_vertex_input(raw: RawVertexInput) -> VertexInput {
    return VertexInput {
        position: raw.pos.xyz,
        texture: vec2<f32>(raw.tan_u.w, raw.bitan_v.w),
        tangent: raw.tan_u.xyz,
        bi_tangent: raw.bitan_v.xyz,
    };
}
