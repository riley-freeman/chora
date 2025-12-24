struct RawVertexInput {
    /// Contains vertex position
    @location(0) pos: vec4<f32>,

    /// Contains normal coords
    @location(1) norm: vec4<f32>,

    /// Contains tangent coords plus the texture U coordinate
    @location(2) tan_u: vec4<f32>,

    /// Contains tangent coords plus the texture U coordinate
    @location(3) bitan_v: vec4<f32>,
}

struct VertexInput {
    position: vec3<f32>,
    normal: vec3<f32>,
    texture: vec2<f32>,
    tangent: vec3<f32>,
    bi_tangent: vec3<f32>,
}

fn translate_raw_vertex_input(raw: RawVertexInput) -> VertexInput {
    var result: VertexInput;
    result.position = raw.pos.xyz;
    result.normal = raw.norm.xyz;
    result.texture = vec2<f32>(raw.tan_u.w, raw.bitan_v.w);
    result.tangent = raw.tan_u.xyz;
    result.bi_tangent = raw.bitan_v.xyz;
    return result;
}
