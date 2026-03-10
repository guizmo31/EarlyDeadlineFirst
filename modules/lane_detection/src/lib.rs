// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

/// LKA — Lane line detection module.
///
/// Processes a grayscale camera image to detect left and right lane boundaries.
/// Uses column intensity projection to find bright lane markings.
/// Outputs lane coefficients as two 2nd-degree polynomials (a, b, c for each lane).
pub struct LaneDetectionModule {
    image_width: usize,
    image_height: usize,
    threshold: u8,
    initialized: bool,
}

impl Default for LaneDetectionModule {
    fn default() -> Self {
        Self {
            image_width: 640,
            image_height: 480,
            threshold: 150,
            initialized: false,
        }
    }
}

impl EdfModule for LaneDetectionModule {
    fn init(&mut self) {
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }
        let image_data = inputs[0];
        let output = &mut outputs[0];
        output.clear();

        let pixel_count = self.image_width * self.image_height;
        if image_data.len() < pixel_count {
            // Not enough data — output zeros
            output.extend_from_slice(&[0u8; 24]); // 6 f32 zeros
            return;
        }

        // Column intensity projection on lower half of image (road region)
        let road_start = self.image_height / 2;
        let mut col_intensity = vec![0u32; self.image_width];
        for y in road_start..self.image_height {
            for x in 0..self.image_width {
                col_intensity[x] += image_data[y * self.image_width + x] as u32;
            }
        }

        // Find left and right lane peaks (brightest columns)
        let mid = self.image_width / 2;
        let road_rows = (self.image_height - road_start) as u32;
        let thresh = self.threshold as u32 * road_rows;

        let left_peak = col_intensity[..mid]
            .iter()
            .enumerate()
            .filter(|(_, v)| **v > thresh)
            .max_by_key(|(_, v)| **v)
            .map(|(i, _)| i)
            .unwrap_or(mid / 3);

        let right_peak = col_intensity[mid..]
            .iter()
            .enumerate()
            .filter(|(_, v)| **v > thresh)
            .max_by_key(|(_, v)| **v)
            .map(|(i, _)| i + mid)
            .unwrap_or(2 * self.image_width / 3);

        // Represent each lane as a simple polynomial: x = a*y² + b*y + c
        // For simulation, approximate as straight vertical lines (a=0, b=0, c=peak_x)
        let left_a: f32 = 0.0;
        let left_b: f32 = 0.0;
        let left_c: f32 = left_peak as f32;

        let right_a: f32 = 0.0;
        let right_b: f32 = 0.0;
        let right_c: f32 = right_peak as f32;

        // Output: 6 × f32 LE = 24 bytes [left_a, left_b, left_c, right_a, right_b, right_c]
        for coeff in &[left_a, left_b, left_c, right_a, right_b, right_c] {
            output.extend_from_slice(&coeff.to_le_bytes());
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(w) = params.get("image_width").and_then(|v| v.as_u64()) {
            self.image_width = (w as usize).max(64);
        }
        if let Some(h) = params.get("image_height").and_then(|v| v.as_u64()) {
            self.image_height = (h as usize).max(48);
        }
        if let Some(t) = params.get("threshold").and_then(|v| v.as_u64()) {
            self.threshold = (t as u8).max(50);
        }
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "LaneDetection".to_string(),
            version: 1,
            description: "ADAS lane line detector — extracts left/right lane boundary polynomials from grayscale camera image using column intensity projection.".to_string(),
            category: "ADAS".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "image_in".to_string(),
                data_type: "u8[]".to_string(),
                sample_size_bytes: 1,
                description: "Grayscale image frame from LaneCamera (width×height u8 pixels)".to_string(),
                example_values: String::new(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "lane_coeffs".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Lane polynomial coefficients: [left_a, left_b, left_c, right_a, right_b, right_c] — x = a·y² + b·y + c".to_string(),
                example_values: "0.0, 0.0, 213.0, 0.0, 0.0, 426.0".to_string(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "image_width".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(640),
                    description: "Expected input image width".to_string(),
                },
                ConfigParam {
                    name: "image_height".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(480),
                    description: "Expected input image height".to_string(),
                },
                ConfigParam {
                    name: "threshold".to_string(),
                    data_type: "u8".to_string(),
                    default_value: serde_json::json!(150),
                    description: "Minimum pixel intensity to consider as lane marking".to_string(),
                },
            ],
            scheduling_type: SchedulingType::DataDriven,
            timing: TimingInfo {
                wcet_us: 8000,
                bcet_us: 3000,
                typical_us: 5500,
            },
            resources: ResourceInfo {
                stack_size_bytes: 32768,
                static_mem_bytes: 307200,
                requires_fpu: true,
                requires_gpu: false,
            },
            asil_level: AsilLevel::AsilB,
        }
    }
}

edf_core::declare_edf_module!(LaneDetectionModule);
