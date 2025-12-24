struct RawInstanceInput {
    @location(4) model_0: vec4<f32>,
    @location(5) model_1: vec4<f32>,
    @location(6) model_2: vec4<f32>,
    @location(7) model_3: vec4<f32>,
}

struct InstanceInput {
    shader_id: i32,
    position: vec3<f32>,
    model: mat4x4<f32>,
}

fn translate_raw_instance_input(raw: RawInstanceInput) -> InstanceInput {
    var result: InstanceInput;
    result.shader_id = bitcast<i32>(raw.model_3.w);
    result.position = raw.model_3.xyz;

    result.model = mat4x4<f32>(raw.model_0, raw.model_1, raw.model_2, raw.model_3);
    result.model[3][3] = 1.0;
    return result;
}
