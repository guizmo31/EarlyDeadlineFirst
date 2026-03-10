// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

/// LKA — Path planning module.
///
/// Computes a desired steering angle to bring the vehicle back to lane center.
/// Uses a proportional-derivative approach on lateral offset and heading error.
pub struct PathPlannerModule {
    kp_lateral: f32,
    kd_heading: f32,
    max_steer_rad: f32,
    lookahead_m: f32,
    prev_offset: f32,
    initialized: bool,
}

impl Default for PathPlannerModule {
    fn default() -> Self {
        Self {
            kp_lateral: 2.0,
            kd_heading: 1.5,
            max_steer_rad: 0.35, // ~20 degrees
            lookahead_m: 15.0,
            prev_offset: 0.0,
            initialized: false,
        }
    }
}

impl EdfModule for PathPlannerModule {
    fn init(&mut self) {
        self.prev_offset = 0.0;
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }
        let position_data = inputs[0];
        let output = &mut outputs[0];
        output.clear();

        // Expect 2 × f32 = 8 bytes from LanePosition
        if position_data.len() < 8 {
            output.extend_from_slice(&0.0f32.to_le_bytes());
            return;
        }

        let lateral_offset = f32::from_le_bytes([
            position_data[0], position_data[1], position_data[2], position_data[3],
        ]);
        let heading_angle = f32::from_le_bytes([
            position_data[4], position_data[5], position_data[6], position_data[7],
        ]);

        // PD controller: desired_steer = Kp * lateral_offset + Kd * heading_error
        let desired_steer = self.kp_lateral * lateral_offset + self.kd_heading * heading_angle;

        // Clamp to physical steering limits
        let clamped_steer = desired_steer.clamp(-self.max_steer_rad, self.max_steer_rad);

        self.prev_offset = lateral_offset;

        // Output: 1 × f32 = 4 bytes [desired_steering_angle_rad]
        output.extend_from_slice(&clamped_steer.to_le_bytes());
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(v) = params.get("kp_lateral").and_then(|v| v.as_f64()) {
            self.kp_lateral = (v as f32).max(0.1).min(10.0);
        }
        if let Some(v) = params.get("kd_heading").and_then(|v| v.as_f64()) {
            self.kd_heading = (v as f32).max(0.0).min(10.0);
        }
        if let Some(v) = params.get("max_steer_rad").and_then(|v| v.as_f64()) {
            self.max_steer_rad = (v as f32).max(0.05).min(0.7);
        }
        if let Some(v) = params.get("lookahead_m").and_then(|v| v.as_f64()) {
            self.lookahead_m = (v as f32).max(5.0).min(50.0);
        }
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "PathPlanner".to_string(),
            version: 1,
            description: "ADAS path planner — computes desired steering angle using PD control on lateral offset and heading error for lane centering.".to_string(),
            category: "ADAS".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "position_in".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Vehicle lane position from LanePosition: [lateral_offset_m, heading_angle_rad]".to_string(),
                example_values: "0.05, 0.01".to_string(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "steer_angle_out".to_string(),
                data_type: "f32".to_string(),
                sample_size_bytes: 4,
                description: "Desired steering angle in radians (positive = steer right)".to_string(),
                example_values: "0.115".to_string(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "kp_lateral".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(2.0),
                    description: "Proportional gain on lateral offset".to_string(),
                },
                ConfigParam {
                    name: "kd_heading".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(1.5),
                    description: "Derivative gain on heading angle error".to_string(),
                },
                ConfigParam {
                    name: "max_steer_rad".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(0.35),
                    description: "Maximum steering angle limit in radians (~20°)".to_string(),
                },
                ConfigParam {
                    name: "lookahead_m".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(15.0),
                    description: "Look-ahead distance for path planning in meters".to_string(),
                },
            ],
            scheduling_type: SchedulingType::DataDriven,
            timing: TimingInfo {
                wcet_us: 1000,
                bcet_us: 200,
                typical_us: 500,
            },
            resources: ResourceInfo {
                stack_size_bytes: 8192,
                static_mem_bytes: 2048,
                requires_fpu: true,
                requires_gpu: false,
            },
            asil_level: AsilLevel::AsilB,
        }
    }
}

edf_core::declare_edf_module!(PathPlannerModule);
