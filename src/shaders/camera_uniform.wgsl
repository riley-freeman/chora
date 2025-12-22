struct CameraUniform {
    view: mat4x4<f32>,
}

@group(0) @binding(1)
var<uniform> camera: CameraUniform;

