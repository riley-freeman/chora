use crate::shader_parser::ParsedShader;
use std::collections::HashMap;
use regex::Regex;

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
        texture_index_map: &HashMap<String, u32>,
        atlas_binding_map: &HashMap<u32, u32>,
    ) -> String {
        let mut rewritten = String::new();

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
                &func.full_text,
                texture_index_map,
                atlas_binding_map,
            ));
            rewritten.push_str("\n\n");
        }

        // 5. Rewrite entry points (vs_main, fs_main)
        if let Some(vs) = &shader.vertex_entry {
            rewritten.push_str(&Self::rewrite_function_body(
                &vs.full_text,
                texture_index_map,
                atlas_binding_map,
            ));
            rewritten.push_str("\n\n");
        }

        if let Some(fs) = &shader.fragment_entry {
            rewritten.push_str(&Self::rewrite_function_body(
                &fs.full_text,
                texture_index_map,
                atlas_binding_map,
            ));
            rewritten.push_str("\n\n");
        }

        rewritten
    }

    /// Rewrite function body to replace textureSample(tex_var, ...) with textureSample#(atlas_N, ...)
    fn rewrite_function_body(
        function_text: &str,
        texture_index_map: &HashMap<String, u32>,
        atlas_binding_map: &HashMap<u32, u32>,
    ) -> String {
        let mut result = function_text.to_string();

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
            &texture_index_map,
            &atlas_binding_map,
        );

        // Should replace textureSample(my_texture with textureSample0(atlas_0
        assert!(rewritten.contains("textureSample0(atlas_0"));
        assert!(!rewritten.contains("textureSample(my_texture"));

        // Should keep sampler binding
        assert!(rewritten.contains("my_sampler"));

        // Should not have texture binding
        assert!(!rewritten.contains("var my_texture"));
    }
}
