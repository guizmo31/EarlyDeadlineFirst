// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

/// Signature of the factory function every plugin DLL must export.
///
/// Returns a raw pointer to a heap-allocated `Box<dyn EdfModule>` (double-boxed for thin pointer).
/// The host takes ownership via `Box::from_raw`.
///
/// # Safety
/// Both host and plugin MUST be compiled with the same rustc version
/// and the same `edf-core` crate version to guarantee ABI compatibility.
pub type CreateModuleFn = unsafe extern "C" fn() -> *mut u8;

/// Signature for the metadata query function.
/// Returns a JSON-serialized `ModuleMetadata` as a heap-allocated byte buffer.
pub type GetMetadataFn = unsafe extern "C" fn(out_len: *mut usize) -> *mut u8;

/// API version constant — checked at load time to detect mismatches.
pub const EDF_PLUGIN_API_VERSION: u32 = 1;

/// Signature for the API version check function.
pub type GetApiVersionFn = unsafe extern "C" fn() -> u32;

/// Macro that module authors invoke to generate the required DLL exports.
///
/// Usage: `edf_core::declare_edf_module!(GainModule);`
///
/// This generates four `extern "C"` functions:
/// - `edf_plugin_api_version` — returns the API version
/// - `edf_module_create` — returns a boxed `dyn EdfModule` (double-boxed for thin pointer)
/// - `edf_module_metadata` — returns JSON-serialized metadata
/// - `edf_module_free_string` — frees a string buffer allocated by the plugin
#[macro_export]
macro_rules! declare_edf_module {
    ($module_type:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn edf_plugin_api_version() -> u32 {
            $crate::EDF_PLUGIN_API_VERSION
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn edf_module_create() -> *mut u8 {
            let module: Box<dyn $crate::EdfModule> = Box::new(<$module_type>::default());
            let double_boxed = Box::new(module);
            Box::into_raw(double_boxed) as *mut u8
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn edf_module_metadata(out_len: *mut usize) -> *mut u8 {
            let instance = <$module_type>::default();
            let meta = $crate::EdfModule::metadata(&instance);
            let json = serde_json::to_string(&meta).unwrap_or_default();
            let bytes = json.into_bytes();
            let len = bytes.len();
            let leaked = std::mem::ManuallyDrop::new(bytes);
            unsafe { *out_len = len; }
            leaked.as_ptr() as *mut u8
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn edf_module_free_string(ptr: *mut u8, len: usize) {
            if !ptr.is_null() && len > 0 {
                unsafe {
                    let _ = Vec::from_raw_parts(ptr, len, len);
                }
            }
        }
    };
}
