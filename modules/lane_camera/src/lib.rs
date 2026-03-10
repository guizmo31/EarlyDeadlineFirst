// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

/// LKA — Camera image acquisition module.
///
/// Simulates a front-facing camera sensor that captures a grayscale image frame
/// at a fixed resolution. Outputs raw pixel data as u8 scanlines.
pub struct LaneCameraModule {
    width: usize,
    height: usize,
    exposure_ms: f32,
    frame_counter: u64,
    initialized: bool,
}

impl Default for LaneCameraModule {
    fn default() -> Self {
        Self {
            width: 640,
            height: 480,
            exposure_ms: 10.0,
            frame_counter: 0,
            initialized: false,
        }
    }
}

impl EdfModule for LaneCameraModule {
    fn init(&mut self) {
        self.frame_counter = 0;
        self.initialized = true;
    }

    fn process(&mut self, _inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if outputs.is_empty() {
            return;
        }
        let output = &mut outputs[0];
        output.clear();

        // Simulate a grayscale image frame with synthetic lane pattern.
        // Two bright vertical bands simulate lane markings on a dark road.
        let left_lane = self.width / 3;
        let right_lane = 2 * self.width / 3;
        let lane_width = 8;

        for y in 0..self.height {
            for x in 0..self.width {
                let pixel = if (x >= left_lane && x < left_lane + lane_width)
                    || (x >= right_lane && x < right_lane + lane_width)
                {
                    // Lane markings: bright white (with slight noise from frame counter)
                    200u8.wrapping_add((self.frame_counter & 0x0F) as u8)
                } else if y > self.height / 2 {
                    // Road surface: dark gray
                    40
                } else {
                    // Sky / horizon: medium gray
                    120
                };
                output.push(pixel);
            }
        }

        // Append frame metadata: frame_counter (u64 LE) + timestamp placeholder (u64 LE)
        output.extend_from_slice(&self.frame_counter.to_le_bytes());
        output.extend_from_slice(&0u64.to_le_bytes()); // timestamp placeholder

        self.frame_counter += 1;
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(w) = params.get("width").and_then(|v| v.as_u64()) {
            self.width = (w as usize).max(64).min(1920);
        }
        if let Some(h) = params.get("height").and_then(|v| v.as_u64()) {
            self.height = (h as usize).max(48).min(1080);
        }
        if let Some(e) = params.get("exposure_ms").and_then(|v| v.as_f64()) {
            self.exposure_ms = (e as f32).max(1.0).min(100.0);
        }
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "LaneCamera".to_string(),
            version: 1,
            description: "ADAS front camera sensor — captures grayscale image frames with synthetic lane markings for LKA processing.".to_string(),
            category: "ADAS".to_string(),
            input_ports: vec![],
            output_ports: vec![PortDescriptor {
                port_name: "image_out".to_string(),
                data_type: "u8[]".to_string(),
                sample_size_bytes: 1,
                description: "Raw grayscale image frame (width×height u8 pixels) + 16-byte metadata trailer (frame_counter u64 LE + timestamp u64 LE)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "width".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(640),
                    description: "Image width in pixels".to_string(),
                },
                ConfigParam {
                    name: "height".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(480),
                    description: "Image height in pixels".to_string(),
                },
                ConfigParam {
                    name: "exposure_ms".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(10.0),
                    description: "Camera exposure time in milliseconds".to_string(),
                },
            ],
            scheduling_type: SchedulingType::Periodic,
            timing: TimingInfo {
                wcet_us: 5000,
                bcet_us: 2000,
                typical_us: 3500,
            },
            resources: ResourceInfo {
                stack_size_bytes: 16384,
                static_mem_bytes: 614400, // 640*480*2 double buffer
                requires_fpu: false,
                requires_gpu: false,
            },
            asil_level: AsilLevel::AsilB,
        }
    }
}

edf_core::declare_edf_module!(LaneCameraModule);
