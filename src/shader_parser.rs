use regex::Regex;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct GlobalDeclaration {
    pub full_text: String,
    pub group: u32,
    pub binding: u32,
    pub var_name: String,
    pub var_type: String,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub parameters: HashMap<String, Parameter>,
    pub return_type: Option<String>,
    pub body: String,
    pub full_text: String,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name        : String, 
    pub position    : usize,
    pub data_type   : String,
    pub location    : Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedShader {
    pub globals: Vec<GlobalDeclaration>,
    pub functions: Vec<Function>,
    pub vertex_entry: Option<Function>,
    pub fragment_entry: Option<Function>,
    pub texture_references: HashMap<String, u32>, // texture_name -> original binding
}

impl ParsedShader {
    pub fn parse(source: &str) -> Self {
        let mut parsed = ParsedShader {
            globals: Vec::new(),
            functions: Vec::new(),
            vertex_entry: None,
            fragment_entry: None,
            texture_references: HashMap::new(),
        };

        // Parse global variable declarations with @group and @binding
        let global_regex = Regex::new(
            r"@group\((\d+)\)\s*@binding\((\d+)\)\s*var\s+(\w+)\s*:\s*([^;]+);"
        ).unwrap();

        for cap in global_regex.captures_iter(source) {
            let group = cap[1].parse::<u32>().unwrap();
            let binding = cap[2].parse::<u32>().unwrap();
            let var_name = cap[3].to_string();
            let var_type = cap[4].trim().to_string();

            // Track texture references
            if var_type.contains("texture_2d") || var_type.contains("texture_3d") ||
               var_type.contains("texture_cube") {
                parsed.texture_references.insert(var_name.clone(), binding);
            }

            parsed.globals.push(GlobalDeclaration {
                full_text: cap[0].to_string(),
                group,
                binding,
                var_name,
                var_type,
            });
        }

        // Parse functions (including entry points)
        // Simple approach: find "fn name(" and extract until matching closing brace
        parsed.functions = Self::extract_functions(source);

        // Find entry points
        for func in &parsed.functions {
            if func.name == "vs_main" {
                parsed.vertex_entry = Some(func.clone());
            } else if func.name == "fs_main" {
                parsed.fragment_entry = Some(func.clone());
            }
        }

        parsed
    }

    /// Get all non-entry-point functions (helper functions)
    pub fn helper_functions(&self) -> Vec<&Function> {
        self.functions
            .iter()
            .filter(|f| f.name != "vs_main" && f.name != "fs_main")
            .collect()
    }

    /// Get all texture variable names
    pub fn texture_names(&self) -> Vec<&String> {
        self.texture_references.keys().collect()
    }

    /// Extract functions from source by manual brace matching
    fn extract_functions(source: &str) -> Vec<Function> {
        let mut functions = Vec::new();
        let fn_regex = Regex::new(r"\bfn\s+(\w+)\s*\(").unwrap();
        let para_regex = Regex::new(r"(?:(?:(@\w+\(\S+\))\s?)?(\w+): *([^\s,)]+),?)").unwrap();

        for cap in fn_regex.captures_iter(source) {
            let name = cap[1].to_string();
            let start_pos = cap.get(0).unwrap().start();

            // Find the opening brace of the function body
            if let Some(open_brace_pos) = source[start_pos..].find('{') {
                let body_start = start_pos + open_brace_pos;

                // Create a slice for the parameters
                let parameter_slice = &source[start_pos..body_start];
                let mut parameters = HashMap::new();
                for (i, cap) in para_regex.captures_iter(parameter_slice).enumerate() {
                    let name = cap[2].to_string();
                    parameters.insert(
                        name.clone(),
                        Parameter {
                            location: cap.get(1).map(|l| l.as_str().to_string()),
                            name: name,
                            position: i,
                            data_type: cap[3].to_string(),
                        }
                    );
                }

                // Create a slice to find the return type 
                let return_type = if let Some(return_token_pos) = source[start_pos..].find('>') {
                    let return_start = return_token_pos + start_pos + 1;
                    let unfiltered = String::from(&source[return_start..body_start]);
                    Some(unfiltered.chars().filter(|c| c.ne(&' ')).collect())
                } else {
                    None
                };


                // Match braces to find the closing brace
                if let Some(body_end) = Self::find_matching_brace(source, body_start) {
                    let full_text = &source[start_pos..=body_end];
                    let body = &source[body_start+1..body_end];
                    functions.push(Function {
                        name,
                        parameters,
                        return_type,
                        body: body.to_string(),
                        full_text: full_text.to_string(),
                    });
                }
            }
        }

        functions
    }

    /// Find the matching closing brace for an opening brace at the given position
    fn find_matching_brace(source: &str, open_pos: usize) -> Option<usize> {
        let chars: Vec<char> = source.chars().collect();
        if open_pos >= chars.len() || chars[open_pos] != '{' {
            return None;
        }

        let mut depth = 0;
        for (i, &ch) in chars[open_pos..].iter().enumerate() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(open_pos + i);
                    }
                }
                _ => {}
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_function_() {
        let shader = r#"
@vertex
fn vs_main(@builtin(vertex_index) idx: u32, vertex: VertexInput, instance: InstanceInput) -> @builtin(position) vec4<f32> {}

fn dummy(idx: u32, vert: VertexInput, inst: InstanceInput) {}
"#;

        let parsed = ParsedShader::parse(shader);
        assert_eq!(parsed.functions.len(), 2);
        assert!(parsed.vertex_entry.is_some());

        {
            assert_eq!(parsed.functions[0].name, "vs_main");
            assert_eq!(parsed.functions[0].parameters.len(), 3);
            assert_eq!(parsed.functions[0].return_type, Some("@builtin(position)vec4<f32>".to_string()));

            assert_eq!(parsed.functions[0].parameters.get("idx").unwrap().location, Some("@builtin(vertex_index)".to_string()));
            assert_eq!(parsed.functions[0].parameters.get("vertex").unwrap().location, None);
            assert_eq!(parsed.functions[0].parameters.get("instance").unwrap().location, None);

            assert_eq!(parsed.functions[0].parameters.get("idx").unwrap().data_type, "u32");
            assert_eq!(parsed.functions[0].parameters.get("vertex").unwrap().data_type, "VertexInput");
            assert_eq!(parsed.functions[0].parameters.get("instance").unwrap().data_type, "InstanceInput");
        }

        {
            assert_eq!(parsed.functions[1].name, "dummy");
            assert_eq!(parsed.functions[1].parameters.len(), 3);
            assert_eq!(parsed.functions[1].return_type, None);

            assert_eq!(parsed.functions[1].parameters.get("idx").unwrap().location, None);
            assert_eq!(parsed.functions[1].parameters.get("vert").unwrap().location, None);
            assert_eq!(parsed.functions[1].parameters.get("inst").unwrap().location, None);

            assert_eq!(parsed.functions[1].parameters.get("idx").unwrap().data_type, "u32");
            assert_eq!(parsed.functions[1].parameters.get("vert").unwrap().data_type, "VertexInput");
            assert_eq!(parsed.functions[1].parameters.get("inst").unwrap().data_type, "InstanceInput");
        }
    }

    #[test]
    fn test_parse_simple_shader() {
        let shader = r#"
@group(1) @binding(0) var my_texture: texture_2d<f32>;
@group(1) @binding(1) var my_sampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return textureSample(my_texture, my_sampler, uv);
}
"#;

        let parsed = ParsedShader::parse(shader);

        assert_eq!(parsed.globals.len(), 2);
        assert_eq!(parsed.texture_references.len(), 1);
        assert!(parsed.texture_references.contains_key("my_texture"));
        assert!(parsed.vertex_entry.is_some());
        assert!(parsed.fragment_entry.is_some());
    }

    #[test]
    fn test_parse_multiple_textures() {
        let shader = r#"
@group(1) @binding(0) var texture_a: texture_2d<f32>;
@group(1) @binding(1) var texture_b: texture_2d<f32>;
@group(1) @binding(2) var texture_c: texture_2d<f32>;
@group(1) @binding(10) var my_sampler: sampler;

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0);
}
"#;

        let parsed = ParsedShader::parse(shader);

        assert_eq!(parsed.texture_references.len(), 3);
        assert_eq!(parsed.texture_references.get("texture_a"), Some(&0));
        assert_eq!(parsed.texture_references.get("texture_b"), Some(&1));
        assert_eq!(parsed.texture_references.get("texture_c"), Some(&2));
    }
}
