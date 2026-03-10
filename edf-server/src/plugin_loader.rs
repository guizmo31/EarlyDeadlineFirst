// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{EdfModule, ModuleMetadata, EDF_PLUGIN_API_VERSION};
use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// A loaded plugin DLL.
struct LoadedPlugin {
    _library: Library,
    metadata: ModuleMetadata,
    path: PathBuf,
}

// Safety: Library holds a DLL handle; the symbols it exposes are Send+Sync
// because our plugin functions are stateless queries or factory functions.
unsafe impl Send for LoadedPlugin {}
unsafe impl Sync for LoadedPlugin {}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_uppercase() && i > 0 {
            let prev = chars[i - 1];
            let next_is_lower = chars.get(i + 1).map_or(false, |c| c.is_lowercase());
            // Insert underscore before uppercase if:
            // - previous char is lowercase (camelCase boundary), OR
            // - previous char is uppercase AND next char is lowercase (acronym end: "AMRCodec" -> "amr_codec")
            if prev.is_lowercase() || (prev.is_uppercase() && next_is_lower) {
                result.push('_');
            }
        }
        result.push(ch.to_lowercase().next().unwrap_or(ch));
    }
    result
}

/// Thread-safe registry of dynamically loaded modules.
pub struct PluginRegistry {
    plugins: RwLock<HashMap<String, LoadedPlugin>>,
    plugins_dir: PathBuf,
}

impl PluginRegistry {
    pub fn new(plugins_dir: PathBuf) -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            plugins_dir,
        }
    }

    /// Scan the plugins directory and load all DLL files.
    /// Returns the names of newly loaded modules.
    pub fn scan_and_load(&self) -> Vec<String> {
        let mut loaded = Vec::new();
        let entries = match std::fs::read_dir(&self.plugins_dir) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("  [plugin_loader] Cannot read plugins dir {:?}: {}", self.plugins_dir, e);
                return loaded;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "dll" && ext != "so" && ext != "dylib" {
                continue;
            }

            // Skip if already loaded
            let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            {
                let plugins = self.plugins.read().unwrap();
                if plugins.values().any(|p| p.path.file_name().unwrap_or_default() == file_name.as_str()) {
                    continue;
                }
            }

            match self.load_plugin(&path) {
                Ok((name, plugin)) => {
                    println!("  [plugin_loader] Loaded: {} from {:?}", name, path);
                    let mut plugins = self.plugins.write().unwrap();
                    loaded.push(name.clone());
                    plugins.insert(name, plugin);
                }
                Err(e) => {
                    eprintln!("  [plugin_loader] Failed to load {:?}: {}", path, e);
                }
            }
        }

        loaded
    }

    fn load_plugin(&self, path: &Path) -> Result<(String, LoadedPlugin), String> {
        let abs_path = std::fs::canonicalize(path)
            .map_err(|e| format!("Cannot canonicalize {:?}: {}", path, e))?;

        unsafe {
            let lib = Library::new(&abs_path)
                .map_err(|e| format!("Library::new failed: {}", e))?;

            // Check API version
            let get_version: Symbol<unsafe extern "C" fn() -> u32> = lib
                .get(b"edf_plugin_api_version")
                .map_err(|e| format!("Symbol edf_plugin_api_version not found: {}", e))?;
            let version = get_version();
            if version != EDF_PLUGIN_API_VERSION {
                return Err(format!(
                    "API version mismatch: plugin={}, host={}",
                    version, EDF_PLUGIN_API_VERSION
                ));
            }

            // Get metadata via JSON serialization (safe across FFI)
            let get_meta: Symbol<unsafe extern "C" fn(*mut usize) -> *mut u8> = lib
                .get(b"edf_module_metadata")
                .map_err(|e| format!("Symbol edf_module_metadata not found: {}", e))?;

            let free_string: Symbol<unsafe extern "C" fn(*mut u8, usize)> = lib
                .get(b"edf_module_free_string")
                .map_err(|e| format!("Symbol edf_module_free_string not found: {}", e))?;

            let mut json_len: usize = 0;
            let json_ptr = get_meta(&mut json_len as *mut usize);
            if json_ptr.is_null() || json_len == 0 {
                return Err("edf_module_metadata returned null".to_string());
            }

            let json_slice = std::slice::from_raw_parts(json_ptr, json_len);
            let json_str = std::str::from_utf8(json_slice)
                .map_err(|e| format!("Invalid UTF-8 in metadata: {}", e))?;
            let metadata: ModuleMetadata = serde_json::from_str(json_str)
                .map_err(|e| format!("Invalid metadata JSON: {}", e))?;

            // Free the string in the plugin's allocator
            free_string(json_ptr, json_len);

            let name = metadata.name.clone();
            Ok((
                name,
                LoadedPlugin {
                    _library: lib,
                    metadata,
                    path: abs_path,
                },
            ))
        }
    }

    /// Create a new module instance by name.
    pub fn create(&self, name: &str) -> Option<Box<dyn EdfModule>> {
        let plugins = self.plugins.read().unwrap();
        let plugin = plugins.get(name)?;

        unsafe {
            let create_fn: Symbol<unsafe extern "C" fn() -> *mut u8> = plugin
                ._library
                .get(b"edf_module_create")
                .ok()?;
            let raw = create_fn();
            if raw.is_null() {
                return None;
            }
            // The plugin returned a *mut Box<dyn EdfModule> (double-boxed, thin pointer)
            let double_boxed = Box::from_raw(raw as *mut Box<dyn EdfModule>);
            Some(*double_boxed)
        }
    }

    /// List metadata for all loaded plugins.
    pub fn list_all(&self) -> Vec<ModuleMetadata> {
        self.plugins
            .read()
            .unwrap()
            .values()
            .map(|p| p.metadata.clone())
            .collect()
    }

    /// Check if a module is loaded.
    pub fn has(&self, name: &str) -> bool {
        self.plugins.read().unwrap().contains_key(name)
    }

    /// Get the plugins directory path.
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    /// Unload a plugin by name (releases the DLL handle).
    pub fn unload(&self, name: &str) -> bool {
        let mut plugins = self.plugins.write().unwrap();
        plugins.remove(name).is_some()
    }

    /// Reload: scan for new DLLs not yet loaded.
    pub fn reload_new(&self) -> Vec<String> {
        self.scan_and_load()
    }

    /// Get source file path hint for a plugin.
    /// Looks in modules/<snake_name>/src/lib.rs relative to the project root.
    pub fn source_path(&self, name: &str) -> Option<PathBuf> {
        let snake_name = to_snake_case(name);
        // plugins_dir is <root>/plugins, so parent is project root
        let project_root = self.plugins_dir.parent()?;
        let src_path = project_root.join(format!("modules/{}/src/lib.rs", snake_name));
        if src_path.exists() {
            Some(src_path)
        } else {
            None
        }
    }
}
