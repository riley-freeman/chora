use crate::coordination::TextureRemapping;
use crate::shader_parser::ParsedShader;
use crate::shader_rewriter::ShaderRewriter;
use regex::Regex;
use std::collections::HashSet;

pub struct ShaderMerger;

impl ShaderMerger {
    /// Merge multiple shader variants into a single mega-shader with switch statements
    ///
    /// rewritten_shaders: Shaders that have already been rewritten to use textureSample# functions
    /// texture_remappings: UV coordinate remappings for textureSample# functions
    /// atlas_count: Number of atlas textures to bind
    pub fn merge_shaders(
        rewritten_shaders: &[String],
        parsed_shaders: &[ParsedShader],
        texture_remappings: &[TextureRemapping],
        atlas_count: usize,
    ) -> String {
        let mut merged = String::new();

        // 1. Generate textureSample# functions (coordinate remapping)
        merged.push_str(&Self::generate_texture_sample_functions(texture_remappings));
        merged.push_str("\n");

        // 2. Add atlas texture bindings
        merged.push_str(&ShaderRewriter::generate_atlas_bindings(atlas_count, 0, 1));
        merged.push_str("\n");

        // 3. Merge common global declarations (deduplicate samplers, uniforms, etc.)
        merged.push_str(&Self::merge_common_globals(parsed_shaders));
        merged.push_str("\n");

        // 4. Add instance data structure with shader_id
        merged.push_str(&Self::generate_instance_data_structure());
        merged.push_str("\n");

        // 5. Rename and add each shader variant's functions
        for (i, shader_src) in rewritten_shaders.iter().enumerate() {
            merged.push_str(&Self::rename_entry_points(shader_src, i));
            merged.push_str("\n");
        }

        // 6. Generate master entry points with switch statements
        merged.push_str(&Self::generate_master_vertex_entry(parsed_shaders));
        merged.push_str("\n");

        merged.push_str(&Self::generate_master_fragment_entry(parsed_shaders));
        merged.push_str("\n");

        merged
    }

    /// Generate textureSample# functions with UV coordinate remapping
    fn generate_texture_sample_functions(remappings: &[TextureRemapping]) -> String {
        let mut functions = String::new();

        for remapping in remappings {
            let function_name = format!("textureSample{}", remapping.binding);
            let min_x = remapping.coords.min().x;
            let min_y = remapping.coords.min().y;
            let max_x = remapping.coords.max().x;
            let max_y = remapping.coords.max().y;

            let function = format!(
                "fn {}(tex: texture_2d<f32>, samp: sampler, uv: vec2<f32>) -> vec4<f32> {{\n\
                 \tlet min_uv = vec2<f32>({}, {});\n\
                 \tlet max_uv = vec2<f32>({}, {});\n\
                 \tlet remapped_uv = min_uv + uv * (max_uv - min_uv);\n\
                 \treturn textureSample(tex, samp, remapped_uv);\n\
                 }}\n\n",
                function_name, min_x, min_y, max_x, max_y
            );
            functions.push_str(&function);
        }

        functions
    }

    /// Merge common global declarations (samplers, uniforms) - deduplicate
    fn merge_common_globals(shaders: &[ParsedShader]) -> String {
        let mut seen_globals = HashSet::new();
        let mut merged = String::new();

        for shader in shaders {
            for global in &shader.globals {
                // Skip texture bindings (we have atlases instead)
                if shader.texture_references.contains_key(&global.var_name) {
                    continue;
                }

                // Deduplicate based on group, binding, and type
                let key = format!("{}:{}:{}", global.group, global.binding, global.var_type);
                if !seen_globals.contains(&key) {
                    seen_globals.insert(key);
                    merged.push_str(&global.full_text);
                    merged.push('\n');
                }
            }
        }

        merged
    }

    /// Generate instance data structure with shader_id
    fn generate_instance_data_structure() -> String {
        r#"struct InstanceData {
    shader_id: u32,
}

@group(0) @binding(2) var<storage, read> instance_data: array<InstanceData>;
"#.to_string()
    }

    /// Rename entry points (vs_main -> vs_main_0, fs_main -> fs_main_0, etc.)
    fn rename_entry_points(shader_src: &str, variant_index: usize) -> String {
        let mut result = shader_src.to_string();

        // Rename vs_main -> vs_main_N
        let vs_regex = Regex::new(r"\bvs_main\b").unwrap();
        result = vs_regex
            .replace_all(&result, format!("vs_main_{}", variant_index).as_str())
            .to_string();

        // Rename fs_main -> fs_main_N
        let fs_regex = Regex::new(r"\bfs_main\b").unwrap();
        result = fs_regex
            .replace_all(&result, format!("fs_main_{}", variant_index).as_str())
            .to_string();

        result
    }

    /// Generate master vertex shader entry point with switch statement
    fn generate_master_vertex_entry(shaders: &[ParsedShader]) -> String {
        // Extract vertex entry signature from first shader (assuming all have compatible signatures)
        let first_vs = shaders
            .iter()
            .find_map(|s| s.vertex_entry.as_ref())
            .expect("At least one shader must have a vertex entry point");

        // Extract parameters and return type from the function signature
        let (params, return_type) = Self::extract_function_signature(&first_vs.full_text);

        // Generate switch cases
        let mut switch_cases = String::new();
        for i in 0..shaders.len() {
            switch_cases.push_str(&format!(
                "\t\tcase {}u: {{ return vs_main_{}({}); }}\n",
                i,
                i,
                Self::extract_param_names(&params)
            ));
        }

        format!(
            "@vertex\nfn vs_main({}, @builtin(instance_index) inst_id: u32) -> {} {{\n\
             \tlet shader_id = instance_data[inst_id].shader_id;\n\
             \tswitch (shader_id) {{\n\
             {}\
             \t\tdefault: {{ return {}; }}\n\
             \t}}\n\
             }}\n",
            params,
            return_type,
            switch_cases,
            Self::get_default_vertex_return(&return_type)
        )
    }

    /// Generate master fragment shader entry point with switch statement
    fn generate_master_fragment_entry(shaders: &[ParsedShader]) -> String {
        // Extract fragment entry signature from first shader
        let first_fs = shaders
            .iter()
            .find_map(|s| s.fragment_entry.as_ref())
            .expect("At least one shader must have a fragment entry point");

        // Extract parameters and return type
        let (params, return_type) = Self::extract_function_signature(&first_fs.full_text);

        // Generate switch cases
        let mut switch_cases = String::new();
        for i in 0..shaders.len() {
            switch_cases.push_str(&format!(
                "\t\tcase {}u: {{ return fs_main_{}({}); }}\n",
                i,
                i,
                Self::extract_param_names(&params)
            ));
        }

        format!(
            "@fragment\nfn fs_main({}, @builtin(instance_index) inst_id: u32) -> {} {{\n\
             \tlet shader_id = instance_data[inst_id].shader_id;\n\
             \tswitch (shader_id) {{\n\
             {}\
             \t\tdefault: {{ return vec4<f32>(1.0, 0.0, 1.0, 1.0); }}\n\
             \t}}\n\
             }}\n",
            params, return_type, switch_cases
        )
    }

    /// Extract function signature (parameters and return type)
    fn extract_function_signature(function_text: &str) -> (String, String) {
        let sig_regex = Regex::new(r"fn\s+\w+\s*\(([^)]*)\)(?:\s*->\s*([^{]+))?").unwrap();

        if let Some(cap) = sig_regex.captures(function_text) {
            let params = cap.get(1).map_or("", |m| m.as_str()).trim().to_string();
            let return_type = cap
                .get(2)
                .map_or("", |m| m.as_str())
                .trim()
                .to_string();
            (params, return_type)
        } else {
            ("".to_string(), "".to_string())
        }
    }

    /// Extract just the parameter names from a parameter list
    fn extract_param_names(params: &str) -> String {
        if params.is_empty() {
            return String::new();
        }

        let param_regex = Regex::new(r"(\w+)\s*:").unwrap();
        let names: Vec<String> = param_regex
            .captures_iter(params)
            .map(|cap| cap[1].to_string())
            .collect();

        names.join(", ")
    }

    /// Get default return value for vertex shader
    fn get_default_vertex_return(return_type: &str) -> String {
        if return_type.contains("vec4") {
            "vec4<f32>(0.0, 0.0, 0.0, 1.0)".to_string()
        } else {
            // Fallback
            format!("{}()", return_type)
        }
    }
}
