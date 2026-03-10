// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

/// LKA — Lane position estimation module.
///
/// Computes the vehicle's lateral offset from lane center and heading angle
/// from the lane polynomial coefficients provided by LaneDetection.
pub struct LanePositionModule {
    lane_width_m: f32,
    pixels_per_meter: f32,
    initialized: bool,
}

impl Default for LanePositionModule {
    fn default() -> Self {
        Self {
            lane_width_m: 3.7,
            pixels_per_meter: 100.0,
            initialized: false,
        }
    }
}

impl EdfModule for LanePositionModule {
    fn init(&mut self) {
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }
        let coeffs_data = inputs[0];
        let output = &mut outputs[0];
        output.clear();

        // Expect 6 × f32 = 24 bytes from LaneDetection
        if coeffs_data.len() < 24 {
            // Default: centered, no heading error
            output.extend_from_slice(&0.0f32.to_le_bytes()); // lateral_offset_m
            output.extend_from_slice(&0.0f32.to_le_bytes()); // heading_angle_rad
            return;
        }

        let read_f32 = |offset: usize| -> f32 {
            let bytes = [
                coeffs_data[offset],
                coeffs_data[offset + 1],
                coeffs_data[offset + 2],
                coeffs_data[offset + 3],
            ];
            f32::from_le_bytes(bytes)
        };

        // left_c and right_c are the x-intercepts (pixel positions of lane lines)
        let left_c = read_f32(8);   // left lane x position (pixels)
        let right_c = read_f32(20); // right lane x position (pixels)

        // Lane center in pixels
        let lane_center_px = (left_c + right_c) / 2.0;

        // Assume camera is centered on the vehicle → image center = vehicle position
        // For simulation, compute offset from actual detected center
        let detected_width_px = right_c - left_c;

        // Lateral offset: positive = vehicle is right of center
        let lateral_offset_m = (lane_center_px - (left_c + detected_width_px / 2.0))
            / self.pixels_per_meter;

        // Heading angle from lane polynomial curvature (simplified: use a coefficients)
        let left_a = read_f32(0);
        let right_a = read_f32(12);
        let avg_curvature = (left_a + right_a) / 2.0;
        let heading_angle_rad = avg_curvature * 10.0; // simplified mapping

        // Output: 2 × f32 = 8 bytes [lateral_offset_m, heading_angle_rad]
        output.extend_from_slice(&lateral_offset_m.to_le_bytes());
        output.extend_from_slice(&heading_angle_rad.to_le_bytes());
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(w) = params.get("lane_width_m").and_then(|v| v.as_f64()) {
            self.lane_width_m = (w as f32).max(2.0).min(5.0);
        }
        if let Some(p) = params.get("pixels_per_meter").and_then(|v| v.as_f64()) {
            self.pixels_per_meter = (p as f32).max(10.0).min(500.0);
        }
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "LanePosition".to_string(),
            version: 1,
            description: "ADAS lane position estimator — computes lateral offset (m) and heading angle (rad) from lane polynomial coefficients.".to_string(),
            category: "ADAS".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "lane_coeffs".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Lane polynomial coefficients from LaneDetection (6 × f32 LE)".to_string(),
                example_values: "0.0, 0.0, 213.0, 0.0, 0.0, 426.0".to_string(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "position_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Vehicle lane position: [lateral_offset_m, heading_angle_rad]".to_string(),
                example_values: "0.05, 0.01".to_string(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "lane_width_m".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(3.7),
                    description: "Standard lane width in meters".to_string(),
                },
                ConfigParam {
                    name: "pixels_per_meter".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(100.0),
                    description: "Camera calibration: pixels per meter at road level".to_string(),
                },
            ],
            scheduling_type: SchedulingType::DataDriven,
            timing: TimingInfo {
                wcet_us: 500,
                bcet_us: 100,
                typical_us: 250,
            },
            resources: ResourceInfo {
                stack_size_bytes: 4096,
                static_mem_bytes: 512,
                requires_fpu: true,
                requires_gpu: false,
            },
            asil_level: AsilLevel::AsilB,
        }
    }
}

edf_core::declare_edf_module!(LanePositionModule);
