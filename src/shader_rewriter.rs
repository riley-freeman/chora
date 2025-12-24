use crate::{render_pipeline::RenderPipelineFlags, shader_parser::{Function, Parameter, ParsedShader}};
use std::collections::HashMap;
use regex::Regex;


const CAMERA_UNIFORM_WGSL: &str = include_str!("shaders/camera_uniform.wgsl");
const WORLD_UNIFORM_WGSL: &str = include_str!("shaders/world_uniform.wgsl");
const VERTEX_INPUT_WGSL: &str = include_str!("shaders/vertex_input.wgsl");
const INSTANCE_INPUT_WGSL: &str = include_str!("shaders/instance_input.wgsl");

const VERTEX_FUNCTION_VERTEX_INFO_WGSL: &str = include_str!("shaders/vertex_function_vertex_info.wgsl");
const VERTEX_FUNCTION_INSTANCE_INFO_WGSL: &str = include_str!("shaders/vertex_function_instance_info.wgsl");

pub struct ShaderRewriter;

impl ShaderRewriter {
    /// Rewrite a shader to use textureSample# functions instead of direct texture references
    ///
    /// texture_index_map: Maps texture variable names to their global texture indices
    /// Example: {"texture_a" -> 0, "texture_b" -> 1}
    ///
    /// atlas_binding_map: Maps global texture indices to atlas binding indices
    /// Example: {0 -> 0, 1 -> 0, 2 -> 1} (textures 0,1 use atlas_0, texture 2 uses atlas_1)
    pub fn rewrite_shader(
        shader: &ParsedShader,
        flags: RenderPipelineFlags,
        texture_index_map: &HashMap<String, u32>,
        atlas_binding_map: &HashMap<u32, u32>,
    ) -> String {
        let mut shader = shader.clone();
        
        let mut rewritten = String::new();
        if flags.contains(RenderPipelineFlags::ALLOW_WORLD_UNIFORM) {
            rewritten.push_str(WORLD_UNIFORM_WGSL);
            rewritten.push('\n');
        }

        if flags.contains(RenderPipelineFlags::ALLOW_CAMERA_UNIFORM) {
            rewritten.push_str(CAMERA_UNIFORM_WGSL);
            rewritten.push('\n');
        }


        // Update whether or not to allow the instance parameter
        if let Some(vertex_func) = &mut shader.vertex_entry {
            if !flags.contains(RenderPipelineFlags::OVERRIDE_VERTEX_INPUT) {
                rewritten.push_str(VERTEX_INPUT_WGSL);
                rewritten.push('\n');

                // Remove any user-added vertex parameter (remove both "vertex" and "raw_vertex" to be safe)
                let _ = vertex_func.parameters.remove("vertex");
                let _ = vertex_func.parameters.remove("raw_vertex");
                
                vertex_func.parameters.insert("raw_vertex".to_string(), Parameter {
                    location: None,
                    position: usize::MAX - 1,
                    data_type: String::from("RawVertexInfo"),
                    name: "vertex".to_string(),
                });
            }

            if flags.contains(RenderPipelineFlags::ALLOW_OBJECT_UNIFORM) {
                // Remove raw_instance and instance to be safe when we're adding our own instance parameter
                let _ = vertex_func.parameters.remove("raw_instance");
                let _ = vertex_func.parameters.remove("instance");
                
                rewritten.push_str(INSTANCE_INPUT_WGSL);
                rewritten.push('\n');

                vertex_func.parameters.insert("raw_instance".to_string(), Parameter {
                    location: None,
                    position: usize::MAX,
                    data_type: String::from("RawInstanceInput"),
                    name: "raw_instance".to_string(),
                });
            }
        }
        


        // 1. Remove texture bindings (they'll be replaced with atlas bindings)
        // 2. Keep sampler bindings
        // 3. Keep all other global declarations
        for global in &shader.globals {
            if shader.texture_references.contains_key(&global.var_name) {
                // Skip texture bindings - they'll be replaced with atlas bindings
                continue;
            }
            // Keep samplers and other bindings
            rewritten.push_str(&global.full_text);
            rewritten.push('\n');
        }

        rewritten.push('\n');

        // 4. Add helper functions (but skip vs_main and fs_main)
        for func in shader.helper_functions() {
            rewritten.push_str(&Self::rewrite_function_body(
                &func,
                flags,
                texture_index_map,
                atlas_binding_map,
            ));
            rewritten.push_str("\n\n");
        }

        // 5. Rewrite entry points (vs_main, fs_main)
        if let Some(vs) = &shader.vertex_entry {
            rewritten.push_str(&Self::rewrite_function_body(
                &vs,
                flags,
                texture_index_map,
                atlas_binding_map,
            ));
            rewritten.push_str("\n\n");
        }

        if let Some(fs) = &shader.fragment_entry {
            rewritten.push_str(&Self::rewrite_function_body(
                &fs,
                flags,
                texture_index_map,
                atlas_binding_map,
            ));
            rewritten.push_str("\n\n");
        }

        rewritten
    }

    fn rewrite_function_parameters(parameters: &Vec<Parameter>) -> String {
        let mut output = String::new();
        for (i, p) in parameters.iter().enumerate() {
            if i != 0 { output.push_str(", "); }
            if let Some(loc) = &p.location {
                output.push_str(loc);
            }

            let rendered = format!("{}: {}", p.name, p.data_type);
            output.push_str(&rendered);
        }
        output
    }

    /// Rewrite function body to replace textureSample(tex_var, ...) with textureSample#(atlas_N, ...)
    fn rewrite_function_body(
        function: &Function,
        flags: RenderPipelineFlags,
        texture_index_map: &HashMap<String, u32>,
        atlas_binding_map: &HashMap<u32, u32>,
    ) -> String {
        let name = &function.name;
        let mut body = function.body.clone();
        let return_type = match function.return_type.clone() {
            Some(dt) => dt,
            None => String::from("()")
        };

        // Append vertex info code to vertex function body
        if name == "vs_main" {
            // Append vertex info code only if OVERRIDE_VERTEX_INPUT is not set
            if !flags.contains(RenderPipelineFlags::OVERRIDE_VERTEX_INPUT) {
                if let Some(param) = function.parameters.get("raw_vertex") {
                    if param.data_type == "RawVertexInfo" {
                        body.push_str("\n    ");
                        body.push_str(VERTEX_FUNCTION_VERTEX_INFO_WGSL.trim());
                    }
                }
            }
            
            // Append instance info code only if ALLOW_OBJECT_UNIFORM is set
            if flags.contains(RenderPipelineFlags::ALLOW_OBJECT_UNIFORM) {
                if let Some(param) = function.parameters.get("raw_instance") {
                    if param.data_type == "RawInstanceInput" {
                        body.push_str("\n    ");
                        body.push_str(VERTEX_FUNCTION_INSTANCE_INFO_WGSL.trim());
                    }
                }
            }
        }

        // Get the parameters and make sure they are in the correct order
        let mut parameters: Vec<Parameter> = function.parameters.values().cloned().collect();
        parameters.sort_by_key(|p| p.position);
        
        // render out the parameters
        let parameters = Self::rewrite_function_parameters(&parameters);

        let mut result = format!("fn {} ({}) -> {} {{{}}}", name, parameters, return_type, body);

        // Replace textureSample(texture_name, sampler, uv) with textureSample#(atlas_N, sampler, uv)
        for (tex_name, &tex_index) in texture_index_map {
            let atlas_binding = atlas_binding_map.get(&tex_index).unwrap_or(&0);

            // Match textureSample(texture_name, ...)
            let pattern = format!(r"\btextureSample\s*\(\s*{}\s*,", tex_name);
            let regex = Regex::new(&pattern).unwrap();

            let replacement = format!("textureSample{}(atlas_{},", tex_index, atlas_binding);
            result = regex.replace_all(&result, replacement.as_str()).to_string();
        }

        result
    }

    /// Generate atlas texture bindings
    pub fn generate_atlas_bindings(atlas_count: usize, start_binding: u32, group: u32) -> String {
        let mut bindings = String::new();

        for i in 0..atlas_count {
            bindings.push_str(&format!(
                "@group({}) @binding({}) var atlas_{}: texture_2d<f32>;\n",
                group,
                start_binding + i as u32,
                i
            ));
        }

        bindings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader_parser::ParsedShader;

    #[test]
    fn test_rewrite_simple_shader() {
        let shader_src = r#"
@group(1) @binding(0) var my_texture: texture_2d<f32>;
@group(1) @binding(1) var my_sampler: sampler;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return textureSample(my_texture, my_sampler, uv);
}
"#;

        let parsed = ParsedShader::parse(shader_src);

        let mut texture_index_map = HashMap::new();
        texture_index_map.insert("my_texture".to_string(), 0);

        let mut atlas_binding_map = HashMap::new();
        atlas_binding_map.insert(0, 0); // texture 0 is in atlas 0

        let rewritten = ShaderRewriter::rewrite_shader(
            &parsed,
            RenderPipelineFlags::all(),
            &texture_index_map,
            &atlas_binding_map,
        );

        // Should replace textureSample(my_texture with textureSample0(atlas_0
        assert!(rewritten.contains("textureSample0(atlas_0"));
        assert!(rewritten.contains("WorldUniform"));
        assert!(rewritten.contains("CameraUniform"));
        assert!(!rewritten.contains("textureSample(my_texture"));

        // Should keep sampler binding
        assert!(rewritten.contains("my_sampler"));

        // Should not have texture binding
        assert!(!rewritten.contains("var my_texture"));
    }

    #[test]
    fn test_vertex_and_instance_input_translation() {
        let shader_src = r#"
@vertex
fn vs_main(@builtin(vertex_index) idx: u32, vertex: VertexInput, instance: InstanceInput) -> @builtin(position) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
"#;

        let parsed = ParsedShader::parse(shader_src);

        let texture_index_map = HashMap::new();
        let atlas_binding_map = HashMap::new();

        // Test 1: Default flags - should add vertex translation but not instance
        let rewritten = ShaderRewriter::rewrite_shader(
            &parsed,
            RenderPipelineFlags::empty(),
            &texture_index_map,
            &atlas_binding_map,
        );

        // Should include vertex input structures
        assert!(rewritten.contains("RawVertexInput"));
        assert!(rewritten.contains("VertexInput"));
        assert!(rewritten.contains("translate_raw_vertex_input"));
        
        // Should have raw_vertex parameter
        assert!(rewritten.contains("raw_vertex"));
        
        // Should append vertex translation code
        assert!(rewritten.contains("let vertex = translate_raw_vertex_input(raw_vertex)"));
        
        // Should NOT include instance input structures
        assert!(!rewritten.contains("RawInstanceInput"));
        assert!(!rewritten.contains("translate_raw_instance_input"));
        assert!(rewritten.contains("InstanceInput"));
        
        // Should NOT have raw_instance parameter
        assert!(!rewritten.contains("raw_instance"));
        
        // Should NOT append instance translation code
        assert!(!rewritten.contains("let instance = translate_raw_instance_input(raw_instance)"));

        // Test 2: With ALLOW_OBJECT_UNIFORM - should add both vertex and instance
        let rewritten = ShaderRewriter::rewrite_shader(
            &parsed,
            RenderPipelineFlags::ALLOW_OBJECT_UNIFORM,
            &texture_index_map,
            &atlas_binding_map,
        );

        // Should include both vertex and instance input structures
        assert!(rewritten.contains("RawVertexInput"));
        assert!(rewritten.contains("VertexInput"));
        assert!(rewritten.contains("RawInstanceInput"));
        assert!(rewritten.contains("InstanceInput"));
        
        // Should have both parameters
        assert!(rewritten.contains("raw_vertex"));
        assert!(rewritten.contains("raw_instance"));
        
        // Should append both translation codes
        assert!(rewritten.contains("let vertex = translate_raw_vertex_input(raw_vertex)"));
        assert!(rewritten.contains("let instance = translate_raw_instance_input(raw_instance)"));

        // Test 3: With OVERRIDE_VERTEX_INPUT - should not add vertex translation
        let rewritten = ShaderRewriter::rewrite_shader(
            &parsed,
            RenderPipelineFlags::OVERRIDE_VERTEX_INPUT,
            &texture_index_map,
            &atlas_binding_map,
        );

        // Should NOT include vertex input structures
        assert!(!rewritten.contains("RawVertexInput"));
        assert!(!rewritten.contains("translate_raw_vertex_input"));
        assert!(rewritten.contains("VertexInput"));
        
        // Should NOT have raw_vertex parameter
        assert!(!rewritten.contains("raw_vertex"));
        
        // Should NOT append vertex translation code
        assert!(!rewritten.contains("let vertex = translate_raw_vertex_input(raw_vertex)"));

        // Test 4: With OVERRIDE_VERTEX_INPUT and ALLOW_OBJECT_UNIFORM - should only add instance
        let rewritten = ShaderRewriter::rewrite_shader(
            &parsed,
            RenderPipelineFlags::OVERRIDE_VERTEX_INPUT | RenderPipelineFlags::ALLOW_OBJECT_UNIFORM,
            &texture_index_map,
            &atlas_binding_map,
        );

        // Should NOT include vertex input structures
        assert!(!rewritten.contains("RawVertexInput"));
        assert!(rewritten.contains("VertexInput"));
        
        // Should include instance input structures
        assert!(rewritten.contains("RawInstanceInput"));
        assert!(rewritten.contains("InstanceInput"));
        
        // Should have raw_instance but not raw_vertex
        assert!(!rewritten.contains("raw_vertex"));
        assert!(rewritten.contains("raw_instance"));
        
        // Should append instance translation but not vertex
        assert!(!rewritten.contains("let vertex = translate_raw_vertex_input(raw_vertex)"));
        assert!(rewritten.contains("let instance = translate_raw_instance_input(raw_instance)"));
    }
}
