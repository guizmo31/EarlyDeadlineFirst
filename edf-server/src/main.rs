// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

mod plugin_loader;

use actix_cors::Cors;
use actix_files::Files;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, middleware};
use edf_core::{EdfModule, SchedulerConfig, simulate};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use plugin_loader::PluginRegistry;

type ModuleInstances = Mutex<HashMap<String, Box<dyn EdfModule>>>;

async fn api_simulate(body: web::Json<SchedulerConfig>) -> HttpResponse {
    let result = simulate(&body);
    HttpResponse::Ok().json(result)
}

async fn api_health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

async fn redirect_root(_req: HttpRequest) -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", "/module/"))
        .finish()
}

// ---- Module Library API ----

/// GET /api/modules — list all modules (plugins + dynamic .meta.json files)
async fn api_list_modules(
    modules_src: web::Data<PathBuf>,
    plugin_registry: web::Data<Arc<PluginRegistry>>,
) -> HttpResponse {
    // Start with dynamically loaded plugins
    let mut modules = plugin_registry.list_all();
    let mut known_names: std::collections::HashSet<String> =
        modules.iter().map(|m| m.name.clone()).collect();

    // Scan for dynamic modules (.meta.json files — AI-generated, not yet compiled)
    if let Ok(entries) = std::fs::read_dir(modules_src.as_ref()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json")
                && path.to_string_lossy().ends_with(".meta.json")
            {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(meta) = serde_json::from_str::<edf_core::ModuleMetadata>(&content) {
                        if !known_names.contains(&meta.name) {
                            modules.push(meta);
                        }
                    }
                }
            }
        }
    }

    HttpResponse::Ok().json(modules)
}

/// GET /api/modules/{name} — get module metadata + Rust source code
async fn api_get_module(
    path: web::Path<String>,
    modules_src: web::Data<PathBuf>,
    plugin_registry: web::Data<Arc<PluginRegistry>>,
) -> HttpResponse {
    let name = path.into_inner();
    let snake_name = to_snake_case(&name);

    // Check dynamic plugins first
    let all_plugins = plugin_registry.list_all();
    if let Some(metadata) = all_plugins.into_iter().find(|m| m.name == name) {
        // Try to find source code in modules/<snake_name>/src/lib.rs
        let source_code = plugin_registry.source_path(&name)
            .and_then(|p| std::fs::read_to_string(p).ok());
        return HttpResponse::Ok().json(serde_json::json!({
            "metadata": metadata,
            "source_code": source_code,
        }));
    }

    // Check dynamic modules (.meta.json — AI-generated, not yet compiled)
    let meta_path = modules_src.join(format!("{}.meta.json", snake_name));
    let source_path = modules_src.join(format!("{}.rs", snake_name));

    if let Ok(content) = std::fs::read_to_string(&meta_path) {
        if let Ok(metadata) = serde_json::from_str::<edf_core::ModuleMetadata>(&content) {
            let source_code = std::fs::read_to_string(&source_path).ok();
            return HttpResponse::Ok().json(serde_json::json!({
                "metadata": metadata,
                "source_code": source_code,
            }));
        }
    }

    HttpResponse::NotFound().json(serde_json::json!({"error": "Module not found"}))
}

/// POST /api/modules — create a new module (AI-generated source or template)
async fn api_create_module(
    body: web::Json<serde_json::Value>,
    modules_src: web::Data<PathBuf>,
) -> HttpResponse {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("NewModule");
    let category = body.get("category").and_then(|v| v.as_str()).unwrap_or("Custom");
    let description = body.get("description").and_then(|v| v.as_str()).unwrap_or("");

    let snake_name = to_snake_case(name);
    let file_path = modules_src.join(format!("{}.rs", snake_name));
    let meta_path = modules_src.join(format!("{}.meta.json", snake_name));

    if file_path.exists() {
        return HttpResponse::Conflict().json(serde_json::json!({
            "error": format!("Module file '{}' already exists", file_path.display())
        }));
    }

    // Use AI-generated source if provided, otherwise fall back to template
    let source = match body.get("source").and_then(|v| v.as_str()) {
        Some(ai_source) => ai_source.to_string(),
        None => generate_module_template(name, category),
    };

    // Save .rs source file
    if let Err(e) = std::fs::write(&file_path, &source) {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e.to_string()
        }));
    }

    // Save .meta.json for dynamic discovery (so the module appears in the list immediately)
    let metadata = edf_core::ModuleMetadata {
        name: name.to_string(),
        version: 1,
        description: if description.is_empty() {
            format!("{} module — {} category", name, category)
        } else {
            description.to_string()
        },
        category: category.to_string(),
        input_ports: vec![edf_core::PortDescriptor {
            port_name: "input_0".to_string(),
            data_type: "u8[]".to_string(),
            sample_size_bytes: 1,
            description: "Input port".to_string(),
            example_values: String::new(),
        }],
        output_ports: vec![edf_core::PortDescriptor {
            port_name: "output_0".to_string(),
            data_type: "u8[]".to_string(),
            sample_size_bytes: 1,
            description: "Output port".to_string(),
            example_values: String::new(),
        }],
        config_params: vec![],
        scheduling_type: edf_core::SchedulingType::Periodic,
        timing: edf_core::TimingInfo { wcet_us: 500, bcet_us: 100, typical_us: 300 },
        resources: edf_core::ResourceInfo {
            stack_size_bytes: 4096,
            static_mem_bytes: 1024,
            requires_fpu: false,
            requires_gpu: false,
        },
        asil_level: edf_core::AsilLevel::QM,
    };

    if let Ok(json) = serde_json::to_string_pretty(&metadata) {
        let _ = std::fs::write(&meta_path, json);
    }

    HttpResponse::Ok().json(serde_json::json!({
        "name": name,
        "file": file_path.to_string_lossy(),
        "source": source,
        "note": "Module created! It appears in the list immediately. Compile it as a cdylib plugin and place the DLL in plugins/ for full integration."
    }))
}

fn generate_module_template(name: &str, category: &str) -> String {
    format!(
        r#"// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{{
    AsilLevel, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo, SchedulingType, TimingInfo,
}};

/// {name} module — {category} category.
///
/// TODO: Add module description here.
///
/// # Ports
/// - **input_0** (input): TODO describe input
/// - **output_0** (output): TODO describe output
///
/// # Configuration
/// ```json
/// {{ "param": "value" }}
/// ```
pub struct {name}Module {{
    // TODO: Add internal state fields
    initialized: bool,
}}

impl Default for {name}Module {{
    fn default() -> Self {{
        Self {{
            initialized: false,
        }}
    }}
}}

impl EdfModule for {name}Module {{
    fn init(&mut self) {{
        self.initialized = true;
        // TODO: Initialize module state
    }}

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {{
        if inputs.is_empty() || outputs.is_empty() {{
            return;
        }}
        // TODO: Implement processing logic
        // Read from inputs[0], write to outputs[0]
        outputs[0] = inputs[0].to_vec();
    }}

    fn configure(&mut self, _params: &serde_json::Value) {{
        // TODO: Handle configuration parameters
        // Example: if let Some(val) = params.get("param") {{ ... }}
    }}

    fn reset(&mut self) {{
        self.initialized = false;
        // TODO: Reset module state
    }}

    fn metadata(&self) -> ModuleMetadata {{
        ModuleMetadata {{
            name: "{name}".to_string(),
            version: 1,
            description: "TODO: describe this module".to_string(),
            category: "{category}".to_string(),
            input_ports: vec![PortDescriptor {{
                port_name: "input_0".to_string(),
                data_type: "u8[]".to_string(),
                sample_size_bytes: 1,
                description: "TODO: describe input".to_string(),
            }}],
            output_ports: vec![PortDescriptor {{
                port_name: "output_0".to_string(),
                data_type: "u8[]".to_string(),
                sample_size_bytes: 1,
                description: "TODO: describe output".to_string(),
            }}],
            scheduling_type: SchedulingType::Periodic,
            timing: TimingInfo {{ wcet_us: 500, bcet_us: 100, typical_us: 300 }},
            resources: ResourceInfo {{
                stack_size_bytes: 4096,
                static_mem_bytes: 1024,
                requires_fpu: false,
                requires_gpu: false,
            }},
            asil_level: AsilLevel::QM,
        }}
    }}
}}
"#,
        name = name,
        category = category,
    )
}

// ---- Module Simulation API ----

/// POST /api/modules/{name}/sim/init — create instance and call init()
async fn api_sim_init(
    path: web::Path<String>,
    instances: web::Data<ModuleInstances>,
    plugin_registry: web::Data<Arc<PluginRegistry>>,
) -> HttpResponse {
    let name = path.into_inner();
    let mut module = match plugin_registry.create(&name) {
        Some(m) => m,
        None => return HttpResponse::NotFound().json(serde_json::json!({
            "error": format!("Module '{}' not found. Ensure the plugin DLL is in the plugins/ directory.", name)
        })),
    };
    module.init();
    let meta = module.metadata();
    instances.lock().unwrap().insert(name.clone(), module);
    HttpResponse::Ok().json(serde_json::json!({
        "status": "initialized",
        "module": name,
        "input_ports": meta.input_ports,
        "output_ports": meta.output_ports,
    }))
}

/// POST /api/modules/{name}/sim/configure — call configure() with JSON params
async fn api_sim_configure(
    path: web::Path<String>,
    body: web::Json<serde_json::Value>,
    instances: web::Data<ModuleInstances>,
) -> HttpResponse {
    let name = path.into_inner();
    let mut map = instances.lock().unwrap();
    let module = match map.get_mut(&name) {
        Some(m) => m,
        None => return HttpResponse::BadRequest().json(serde_json::json!({
            "error": format!("Module '{}' not initialized. Call init first.", name)
        })),
    };
    let params = body.get("params").cloned().unwrap_or(body.into_inner());
    module.configure(&params);
    HttpResponse::Ok().json(serde_json::json!({
        "status": "configured",
        "module": name,
    }))
}

/// POST /api/modules/{name}/sim/process — call process() with input data
async fn api_sim_process(
    path: web::Path<String>,
    body: web::Json<serde_json::Value>,
    instances: web::Data<ModuleInstances>,
) -> HttpResponse {
    let name = path.into_inner();
    let mut map = instances.lock().unwrap();
    let module = match map.get_mut(&name) {
        Some(m) => m,
        None => return HttpResponse::BadRequest().json(serde_json::json!({
            "error": format!("Module '{}' not initialized. Call init first.", name)
        })),
    };

    // Parse inputs: array of arrays of u8
    let inputs_raw: Vec<Vec<u8>> = match body.get("inputs") {
        Some(arr) => {
            if let Some(arrays) = arr.as_array() {
                arrays.iter().map(|port| {
                    port.as_array()
                        .unwrap_or(&vec![])
                        .iter()
                        .filter_map(|v| v.as_u64().map(|n| n as u8))
                        .collect()
                }).collect()
            } else {
                vec![]
            }
        }
        None => vec![],
    };

    let input_slices: Vec<&[u8]> = inputs_raw.iter().map(|v| v.as_slice()).collect();
    let meta = module.metadata();
    let num_outputs = meta.output_ports.len();
    let mut outputs: Vec<Vec<u8>> = vec![Vec::new(); num_outputs];

    module.process(&input_slices, &mut outputs);

    // Return outputs as arrays of u8
    let outputs_json: Vec<Vec<u8>> = outputs;
    HttpResponse::Ok().json(serde_json::json!({
        "status": "processed",
        "module": name,
        "outputs": outputs_json,
    }))
}

/// POST /api/modules/{name}/sim/reset — call reset()
async fn api_sim_reset(
    path: web::Path<String>,
    instances: web::Data<ModuleInstances>,
) -> HttpResponse {
    let name = path.into_inner();
    let mut map = instances.lock().unwrap();
    let module = match map.get_mut(&name) {
        Some(m) => m,
        None => return HttpResponse::BadRequest().json(serde_json::json!({
            "error": format!("Module '{}' not initialized. Call init first.", name)
        })),
    };
    module.reset();
    HttpResponse::Ok().json(serde_json::json!({
        "status": "reset",
        "module": name,
    }))
}

// ---- Topologies API ----

/// GET /api/topologies — list all topology files
async fn api_list_topologies(
    topologies_dir: web::Data<PathBuf>,
) -> HttpResponse {
    let mut topologies = Vec::new();
    let dir = topologies_dir.as_ref();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "json" {
                continue;
            }
            let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            // Skip scheduling configs, edf-config and viewer-data files
            if file_name.starts_with("scheduling_")
                || file_name.contains("edf-config")
                || file_name == "viewer-data.json"
            {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(topo) = serde_json::from_str::<serde_json::Value>(&content) {
                    let name = topo.get("name").and_then(|v| v.as_str()).unwrap_or(&file_name);
                    let topo_version = topo
                        .get("topology_version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("1.0");
                    let description = topo.get("description").and_then(|v| v.as_str()).unwrap_or("");
                    let category = topo.get("category").and_then(|v| v.as_str()).unwrap_or("General");
                    let module_count = topo.get("modules").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
                    let instance_count = topo.get("instances").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);

                    // Look for matching scheduling files that reference this topology
                    let mut scheduling_files = Vec::new();
                    if let Ok(sched_entries) = std::fs::read_dir(dir) {
                        for sentry in sched_entries.flatten() {
                            let sname = sentry.file_name().to_string_lossy().to_string();
                            if sname.starts_with("scheduling_") && sname.ends_with(".json") {
                                if let Ok(sc) = std::fs::read_to_string(sentry.path()) {
                                    if let Ok(sv) = serde_json::from_str::<serde_json::Value>(&sc) {
                                        let ref_file = sv.get("topology_file").and_then(|v| v.as_str()).unwrap_or("");
                                        if ref_file == file_name {
                                            scheduling_files.push(sname);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Also check legacy edf-config pairing
                    let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                    let legacy_config = format!("{}-edf-config.json", stem);
                    let has_legacy = dir.join(&legacy_config).exists();

                    topologies.push(serde_json::json!({
                        "file": file_name,
                        "name": name,
                        "topology_version": topo_version,
                        "description": description,
                        "category": category,
                        "module_count": module_count,
                        "instance_count": instance_count,
                        "scheduling_files": scheduling_files,
                        "has_edf_config": has_legacy || !scheduling_files.is_empty(),
                        "edf_config_file": if has_legacy { Some(legacy_config) } else { scheduling_files.first().cloned() },
                    }));
                }
            }
        }
    }
    HttpResponse::Ok().json(topologies)
}

/// GET /api/topologies/{filename} — get a specific topology file
async fn api_get_topology(
    path: web::Path<String>,
    topologies_dir: web::Data<PathBuf>,
) -> HttpResponse {
    let filename = path.into_inner();
    // Prevent path traversal
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid filename"}));
    }
    let file_path = topologies_dir.join(&filename);
    match std::fs::read_to_string(&file_path) {
        Ok(content) => {
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(json) => HttpResponse::Ok().json(json),
                Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
            }
        }
        Err(_) => HttpResponse::NotFound().json(serde_json::json!({"error": "Topology not found"})),
    }
}

/// POST /api/topologies/{filename} — save a topology file
async fn api_save_topology(
    path: web::Path<String>,
    body: web::Json<serde_json::Value>,
    topologies_dir: web::Data<PathBuf>,
) -> HttpResponse {
    let filename = path.into_inner();
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid filename"}));
    }
    if !filename.ends_with(".json") {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "Filename must end with .json"}));
    }
    let file_path = topologies_dir.join(&filename);
    match serde_json::to_string_pretty(&body.into_inner()) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&file_path, &json) {
                return HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()}));
            }
            HttpResponse::Ok().json(serde_json::json!({"status": "saved", "file": filename}))
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}

/// POST /api/modules/{name}/save-and-verify — save source, compile, reload, validate
async fn api_save_and_verify(
    path: web::Path<String>,
    body: web::Json<serde_json::Value>,
    modules_src: web::Data<PathBuf>,
    plugin_registry: web::Data<Arc<PluginRegistry>>,
) -> HttpResponse {
    let name = path.into_inner();
    let snake_name = to_snake_case(&name);
    let source = match body.get("source").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Missing 'source' field"})),
    };

    // Save source to modules/<snake_name>/src/lib.rs
    let src_path = modules_src.join(&snake_name).join("src").join("lib.rs");
    if !src_path.parent().map(|p| p.exists()).unwrap_or(false) {
        return HttpResponse::NotFound().json(serde_json::json!({
            "error": format!("Module directory not found: modules/{}/src/", snake_name)
        }));
    }
    if let Err(e) = std::fs::write(&src_path, source) {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to write source: {}", e)
        }));
    }

    // Read package name from Cargo.toml
    let cargo_toml_path = modules_src.join(&snake_name).join("Cargo.toml");
    let pkg_name = match std::fs::read_to_string(&cargo_toml_path) {
        Ok(content) => {
            content.lines()
                .find(|l| l.trim().starts_with("name"))
                .and_then(|l| {
                    let parts: Vec<&str> = l.splitn(2, '=').collect();
                    parts.get(1).map(|v| v.trim().trim_matches('"').to_string())
                })
                .unwrap_or_else(|| format!("edf-mod-{}", snake_name))
        }
        Err(_) => format!("edf-mod-{}", snake_name),
    };

    // Find workspace root (parent of modules_src which is <root>/modules)
    let workspace_root = modules_src.parent()
        .unwrap_or(modules_src.as_ref())
        .to_path_buf();

    // Unload the existing plugin (releases DLL handle on Windows)
    let was_loaded = plugin_registry.unload(&name);
    if was_loaded {
        println!("  [save-and-verify] Unloaded plugin '{}' for recompilation", name);
    }

    // Compile
    let output = std::process::Command::new("cargo")
        .args(["build", "--release", "-p", &pkg_name])
        .current_dir(&workspace_root)
        .output();

    match output {
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr).to_string();

            if result.status.success() {
                // Find compiled DLL and copy to plugins/
                let dll_name = format!("{}.dll", pkg_name.replace('-', "_"));
                let plugins_dir = plugin_registry.plugins_dir().to_path_buf();
                let target_dir = workspace_root.join("target");

                let mut found_dll = None;
                for sub in ["x86_64-pc-windows-gnu/release", "release", "x86_64-pc-windows-msvc/release"] {
                    let candidate = target_dir.join(sub).join(&dll_name);
                    if candidate.exists() {
                        found_dll = Some(candidate);
                        break;
                    }
                }

                if let Some(dll_path) = found_dll {
                    let dest = plugins_dir.join(&dll_name);
                    if let Err(e) = std::fs::copy(&dll_path, &dest) {
                        return HttpResponse::Ok().json(serde_json::json!({
                            "status": "compiled",
                            "warning": format!("Compiled but failed to copy DLL: {}", e),
                            "compilation_output": stderr,
                        }));
                    }

                    // Reload the plugin
                    let reloaded = plugin_registry.reload_new();

                    // Get updated metadata
                    let metadata = plugin_registry.list_all().into_iter()
                        .find(|m| m.name == name);

                    // Validate metadata
                    let mut warnings = Vec::new();
                    if let Some(ref meta) = metadata {
                        if meta.input_ports.is_empty() && meta.output_ports.is_empty() {
                            warnings.push("Module has no ports declared".to_string());
                        }
                        if meta.description.is_empty() || meta.description.starts_with("TODO") {
                            warnings.push("Module description is missing or placeholder".to_string());
                        }
                    }

                    HttpResponse::Ok().json(serde_json::json!({
                        "status": "verified",
                        "metadata": metadata,
                        "reloaded": reloaded,
                        "warnings": warnings,
                        "compilation_output": stderr,
                    }))
                } else {
                    HttpResponse::Ok().json(serde_json::json!({
                        "status": "compiled",
                        "warning": format!("Compiled but DLL '{}' not found in target/", dll_name),
                        "compilation_output": stderr,
                    }))
                }
            } else {
                HttpResponse::Ok().json(serde_json::json!({
                    "status": "error",
                    "compilation_output": stderr,
                }))
            }
        }
        Err(e) => {
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to run cargo: {}", e)
            }))
        }
    }
}

/// POST /api/modules/create-manual — create a module crate from wizard data
async fn api_create_module_manual(
    body: web::Json<serde_json::Value>,
    modules_src: web::Data<PathBuf>,
) -> HttpResponse {
    let name = match body.get("name").and_then(|v| v.as_str()) {
        Some(n) if !n.is_empty() => n,
        _ => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Name is required"})),
    };
    let category = body.get("category").and_then(|v| v.as_str()).unwrap_or("Custom");
    let description = body.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let asil = body.get("asil_level").and_then(|v| v.as_str()).unwrap_or("QM");
    let scheduling = body.get("scheduling_type").and_then(|v| v.as_str()).unwrap_or("Periodic");
    let input_ports = body.get("input_ports").and_then(|v| v.as_array());
    let output_ports = body.get("output_ports").and_then(|v| v.as_array());

    let snake_name = to_snake_case(name);
    let mod_dir = modules_src.join(&snake_name);

    if mod_dir.exists() {
        return HttpResponse::Conflict().json(serde_json::json!({
            "error": format!("Module '{}' already exists at modules/{}/", name, snake_name)
        }));
    }

    // Create directory structure
    let src_dir = mod_dir.join("src");
    if let Err(e) = std::fs::create_dir_all(&src_dir) {
        return HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()}));
    }

    // Generate Cargo.toml
    let cargo_toml = format!(
        r#"[package]
name = "edf-mod-{snake_name}"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
edf-core = {{ git = "https://github.com/guizmo31/EarlyDeadlineFirst.git" }}
serde_json = "1"
"#,
        snake_name = snake_name
    );
    if let Err(e) = std::fs::write(mod_dir.join("Cargo.toml"), &cargo_toml) {
        return HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()}));
    }

    // Generate lib.rs from wizard data
    let source = generate_manual_module_source(
        name, category, description, asil, scheduling,
        input_ports, output_ports,
    );
    if let Err(e) = std::fs::write(src_dir.join("lib.rs"), &source) {
        return HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()}));
    }

    // Add to workspace Cargo.toml
    let workspace_root = modules_src.parent().unwrap_or(modules_src.as_ref());
    let workspace_toml_path = workspace_root.join("Cargo.toml");
    if let Ok(content) = std::fs::read_to_string(&workspace_toml_path) {
        let member_entry = format!("\"modules/{}\"", snake_name);
        if !content.contains(&member_entry) {
            let new_content = content.replace(
                "\n]\nresolver",
                &format!("\n    {},\n]\nresolver", member_entry),
            );
            let _ = std::fs::write(&workspace_toml_path, new_content);
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "status": "created",
        "name": name,
        "path": format!("modules/{}/src/lib.rs", snake_name),
        "source": source,
    }))
}

fn generate_manual_module_source(
    name: &str,
    category: &str,
    description: &str,
    asil: &str,
    scheduling: &str,
    input_ports: Option<&Vec<serde_json::Value>>,
    output_ports: Option<&Vec<serde_json::Value>>,
) -> String {
    let empty_vec = vec![];
    let inputs = input_ports.unwrap_or(&empty_vec);
    let outputs = output_ports.unwrap_or(&empty_vec);

    // Build port descriptors code
    let input_ports_code: Vec<String> = inputs.iter().map(|p| {
        let pname = p.get("port_name").and_then(|v| v.as_str()).unwrap_or("input_0");
        let dtype = p.get("data_type").and_then(|v| v.as_str()).unwrap_or("f32[]");
        let size: u64 = p.get("sample_size_bytes").and_then(|v| v.as_u64()).unwrap_or(4);
        let desc = p.get("description").and_then(|v| v.as_str()).unwrap_or("");
        format!(
            r#"                PortDescriptor {{
                    port_name: "{pname}".to_string(),
                    data_type: "{dtype}".to_string(),
                    sample_size_bytes: {size},
                    description: "{desc}".to_string(),
                    example_values: String::new(),
                }}"#
        )
    }).collect();
    let input_ports_str = if input_ports_code.is_empty() {
        "vec![]".to_string()
    } else {
        format!("vec![\n{}\n            ]", input_ports_code.join(",\n"))
    };

    let output_ports_code: Vec<String> = outputs.iter().map(|p| {
        let pname = p.get("port_name").and_then(|v| v.as_str()).unwrap_or("output_0");
        let dtype = p.get("data_type").and_then(|v| v.as_str()).unwrap_or("f32[]");
        let size: u64 = p.get("sample_size_bytes").and_then(|v| v.as_u64()).unwrap_or(4);
        let desc = p.get("description").and_then(|v| v.as_str()).unwrap_or("");
        format!(
            r#"                PortDescriptor {{
                    port_name: "{pname}".to_string(),
                    data_type: "{dtype}".to_string(),
                    sample_size_bytes: {size},
                    description: "{desc}".to_string(),
                    example_values: String::new(),
                }}"#
        )
    }).collect();
    let output_ports_str = if output_ports_code.is_empty() {
        "vec![]".to_string()
    } else {
        format!("vec![\n{}\n            ]", output_ports_code.join(",\n"))
    };

    let asil_variant = match asil {
        "ASIL-A" => "AsilA",
        "ASIL-B" => "AsilB",
        "ASIL-C" => "AsilC",
        "ASIL-D" => "AsilD",
        _ => "QM",
    };

    let num_inputs = inputs.len();
    let num_outputs = outputs.len();

    // Build process body
    let process_body = if num_inputs > 0 && num_outputs > 0 {
        "        if !inputs.is_empty() && !outputs.is_empty() {\n            // TODO: Implement processing logic\n            outputs[0] = inputs[0].to_vec();\n        }".to_string()
    } else if num_outputs > 0 {
        "        // TODO: Generate output data\n        outputs[0] = vec![0u8; 4];".to_string()
    } else {
        "        // TODO: Implement processing logic".to_string()
    };

    format!(
        r#"// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{{
    AsilLevel, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo, SchedulingType, TimingInfo,
    ConfigParam,
}};

pub struct {name}Module {{
    initialized: bool,
}}

impl Default for {name}Module {{
    fn default() -> Self {{
        Self {{ initialized: false }}
    }}
}}

impl EdfModule for {name}Module {{
    fn init(&mut self) {{
        self.initialized = true;
    }}

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {{
{process_body}
    }}

    fn configure(&mut self, _params: &serde_json::Value) {{
        // TODO: Handle configuration parameters
    }}

    fn reset(&mut self) {{
        self.initialized = false;
    }}

    fn metadata(&self) -> ModuleMetadata {{
        ModuleMetadata {{
            name: "{name}".to_string(),
            version: 1,
            description: "{description}".to_string(),
            category: "{category}".to_string(),
            input_ports: {input_ports_str},
            output_ports: {output_ports_str},
            config_params: vec![],
            scheduling_type: SchedulingType::{scheduling},
            timing: TimingInfo {{ wcet_us: 500, bcet_us: 100, typical_us: 300 }},
            resources: ResourceInfo {{
                stack_size_bytes: 4096,
                static_mem_bytes: 1024,
                requires_fpu: false,
                requires_gpu: false,
            }},
            asil_level: AsilLevel::{asil_variant},
        }}
    }}
}}

edf_core::declare_edf_module!({name}Module);
"#,
        name = name,
        description = description,
        category = category,
        scheduling = scheduling,
        asil_variant = asil_variant,
        input_ports_str = input_ports_str,
        output_ports_str = output_ports_str,
        process_body = process_body,
    )
}

/// POST /api/modules/reload — scan for new plugin DLLs
async fn api_reload_plugins(
    plugin_registry: web::Data<Arc<PluginRegistry>>,
) -> HttpResponse {
    let new_modules = plugin_registry.reload_new();
    HttpResponse::Ok().json(serde_json::json!({
        "reloaded": new_modules,
        "total": plugin_registry.list_all().len(),
    }))
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_uppercase() && i > 0 {
            let prev = chars[i - 1];
            let next_is_lower = chars.get(i + 1).map_or(false, |c| c.is_lowercase());
            if prev.is_lowercase() || (prev.is_uppercase() && next_is_lower) {
                result.push('_');
            }
        }
        result.push(ch.to_lowercase().next().unwrap_or(ch));
    }
    result
}

// ---- Directory helpers ----

fn find_dir_with_index(candidates: &[Option<PathBuf>], fallback: &str) -> PathBuf {
    for candidate in candidates.iter().flatten() {
        if candidate.join("index.html").exists() {
            return candidate.clone();
        }
    }
    PathBuf::from(fallback)
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn find_web_dir() -> PathBuf {
    let d = exe_dir();
    find_dir_with_index(&[
        d.as_ref().map(|d| d.join("../../edf-web")),
        d.as_ref().map(|d| d.join("../../../edf-web")),
        Some(PathBuf::from("./edf-web")),
        Some(PathBuf::from("../edf-web")),
    ], "./edf-web")
}

fn find_viewer_dir() -> PathBuf {
    let d = exe_dir();
    find_dir_with_index(&[
        d.as_ref().map(|d| d.join("../../../edf-viewer")),
        Some(PathBuf::from("./edf-viewer")),
        Some(PathBuf::from("../edf-viewer")),
    ], "./edf-viewer")
}

fn find_builder_dir() -> PathBuf {
    let d = exe_dir();
    find_dir_with_index(&[
        d.as_ref().map(|d| d.join("../../../edf-builder")),
        Some(PathBuf::from("./edf-builder")),
        Some(PathBuf::from("../edf-builder")),
    ], "./edf-builder")
}

fn find_module_dir() -> PathBuf {
    let d = exe_dir();
    find_dir_with_index(&[
        d.as_ref().map(|d| d.join("../../../edf-module")),
        Some(PathBuf::from("./edf-module")),
        Some(PathBuf::from("../edf-module")),
    ], "./edf-module")
}

fn find_plugins_dir() -> PathBuf {
    let d = exe_dir();
    // Check common locations relative to the executable
    for candidate in [
        d.as_ref().map(|d| d.join("../../../plugins")),
        d.as_ref().map(|d| d.join("../../plugins")),
        Some(PathBuf::from("./plugins")),
        Some(PathBuf::from("../plugins")),
    ]
    .iter()
    .flatten()
    {
        if candidate.is_dir() {
            return candidate.clone();
        }
    }
    // Default: create alongside the project root
    d.as_ref()
        .map(|d| d.join("../../../plugins"))
        .unwrap_or_else(|| PathBuf::from("./plugins"))
}

fn find_topologies_dir() -> PathBuf {
    let d = exe_dir();
    for candidate in [
        d.as_ref().map(|d| d.join("../../../Topologies")),
        d.as_ref().map(|d| d.join("../../Topologies")),
        Some(PathBuf::from("./Topologies")),
        Some(PathBuf::from("../Topologies")),
    ]
    .iter()
    .flatten()
    {
        if candidate.is_dir() {
            return candidate.clone();
        }
    }
    d.as_ref()
        .map(|d| d.join("../../../Topologies"))
        .unwrap_or_else(|| PathBuf::from("./Topologies"))
}

fn find_shared_dir() -> PathBuf {
    let d = exe_dir();
    for candidate in [
        d.as_ref().map(|d| d.join("../../../edf-shared")),
        Some(PathBuf::from("./edf-shared")),
        Some(PathBuf::from("../edf-shared")),
    ]
    .iter()
    .flatten()
    {
        if candidate.is_dir() {
            return candidate.clone();
        }
    }
    d.as_ref()
        .map(|d| d.join("../../../edf-shared"))
        .unwrap_or_else(|| PathBuf::from("./edf-shared"))
}

fn find_modules_src_dir() -> PathBuf {
    let d = exe_dir();
    for candidate in [
        d.as_ref().map(|d| d.join("../../../modules")),
        d.as_ref().map(|d| d.join("../../modules")),
        Some(PathBuf::from("./modules")),
        Some(PathBuf::from("../modules")),
    ]
    .iter()
    .flatten()
    {
        if candidate.is_dir() {
            return candidate.clone();
        }
    }
    d.as_ref()
        .map(|d| d.join("../../../modules"))
        .unwrap_or_else(|| PathBuf::from("./modules"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let web_dir = find_web_dir();
    let viewer_dir = find_viewer_dir();
    let builder_dir = find_builder_dir();
    let module_dir = find_module_dir();
    let shared_dir = find_shared_dir();
    let modules_src_dir = find_modules_src_dir();
    let plugins_dir = find_plugins_dir();
    let topologies_dir = find_topologies_dir();
    std::fs::create_dir_all(&plugins_dir).ok();
    std::fs::create_dir_all(&topologies_dir).ok();

    println!("EDF Scheduler Server starting on http://localhost:8080");
    println!("  /scheduler   -> {}", web_dir.display());
    println!("  /builder     -> {}", builder_dir.display());
    println!("  /viewer      -> {}", viewer_dir.display());
    println!("  /module      -> {}", module_dir.display());
    println!("  /shared      -> {}", shared_dir.display());
    println!("  modules src  : {}", modules_src_dir.display());
    println!("  plugins dir  : {}", plugins_dir.display());
    println!("  topologies   : {}", topologies_dir.display());

    // Load dynamic module plugins
    let plugin_registry = Arc::new(PluginRegistry::new(plugins_dir));
    let loaded = plugin_registry.scan_and_load();
    println!("  Loaded {} dynamic module plugin(s)", loaded.len());

    let web_dir_str = web_dir.to_string_lossy().to_string();
    let viewer_dir_str = viewer_dir.to_string_lossy().to_string();
    let builder_dir_str = builder_dir.to_string_lossy().to_string();
    let module_dir_str = module_dir.to_string_lossy().to_string();
    let shared_dir_str = shared_dir.to_string_lossy().to_string();
    let modules_src_data = web::Data::new(modules_src_dir);
    let topologies_data = web::Data::new(topologies_dir);
    let module_instances: web::Data<ModuleInstances> =
        web::Data::new(Mutex::new(HashMap::new()));
    let plugin_registry_data = web::Data::new(plugin_registry);

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .wrap(cors)
            .wrap(middleware::Logger::default())
            .wrap(middleware::DefaultHeaders::new()
                .add(("Cache-Control", "no-cache, no-store, must-revalidate")))
            .app_data(modules_src_data.clone())
            .app_data(topologies_data.clone())
            .app_data(module_instances.clone())
            .app_data(plugin_registry_data.clone())
            .route("/", web::get().to(redirect_root))
            .route("/api/health", web::get().to(api_health))
            .route("/api/simulate", web::post().to(api_simulate))
            .route("/api/modules", web::get().to(api_list_modules))
            .route("/api/modules", web::post().to(api_create_module))
            .route("/api/modules/{name}", web::get().to(api_get_module))
            .route("/api/modules/{name}/sim/init", web::post().to(api_sim_init))
            .route("/api/modules/{name}/sim/configure", web::post().to(api_sim_configure))
            .route("/api/modules/{name}/sim/process", web::post().to(api_sim_process))
            .route("/api/modules/{name}/sim/reset", web::post().to(api_sim_reset))
            .route("/api/modules/reload", web::post().to(api_reload_plugins))
            .route("/api/modules/create-manual", web::post().to(api_create_module_manual))
            .route("/api/modules/{name}/save-and-verify", web::post().to(api_save_and_verify))
            .route("/api/topologies", web::get().to(api_list_topologies))
            .route("/api/topologies/{filename}", web::get().to(api_get_topology))
            .route("/api/topologies/{filename}", web::post().to(api_save_topology))
            .service(Files::new("/shared", &shared_dir_str).redirect_to_slash_directory())
            .service(Files::new("/builder", &builder_dir_str).index_file("index.html").redirect_to_slash_directory())
            .service(Files::new("/viewer", &viewer_dir_str).index_file("index.html").redirect_to_slash_directory())
            .service(Files::new("/module", &module_dir_str).index_file("index.html").redirect_to_slash_directory())
            .service(Files::new("/scheduler", &web_dir_str).index_file("index.html").redirect_to_slash_directory())
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
