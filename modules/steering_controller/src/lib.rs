// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

/// LKA — PID steering controller module.
///
/// Converts a desired steering angle into a steering torque command
/// using a PID controller with anti-windup. Runs at a high frequency
/// for smooth actuation.
pub struct SteeringControllerModule {
    kp: f32,
    ki: f32,
    kd: f32,
    max_torque_nm: f32,
    integral: f32,
    prev_error: f32,
    dt_s: f32,
    current_angle: f32,
    initialized: bool,
}

impl Default for SteeringControllerModule {
    fn default() -> Self {
        Self {
            kp: 15.0,
            ki: 0.5,
            kd: 2.0,
            max_torque_nm: 5.0,
            integral: 0.0,
            prev_error: 0.0,
            dt_s: 0.01, // 100 Hz control loop
            current_angle: 0.0,
            initialized: false,
        }
    }
}

impl EdfModule for SteeringControllerModule {
    fn init(&mut self) {
        self.integral = 0.0;
        self.prev_error = 0.0;
        self.current_angle = 0.0;
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }
        let steer_data = inputs[0];
        let output = &mut outputs[0];
        output.clear();

        // Expect 1 × f32 = 4 bytes from PathPlanner (desired steering angle)
        if steer_data.len() < 4 {
            output.extend_from_slice(&0.0f32.to_le_bytes());
            return;
        }

        let desired_angle = f32::from_le_bytes([
            steer_data[0], steer_data[1], steer_data[2], steer_data[3],
        ]);

        // PID control
        let error = desired_angle - self.current_angle;

        // Anti-windup: only integrate if not saturated
        let tentative_integral = self.integral + error * self.dt_s;
        if (self.ki * tentative_integral).abs() < self.max_torque_nm {
            self.integral = tentative_integral;
        }

        let derivative = (error - self.prev_error) / self.dt_s;

        let torque = self.kp * error + self.ki * self.integral + self.kd * derivative;
        let clamped_torque = torque.clamp(-self.max_torque_nm, self.max_torque_nm);

        self.prev_error = error;

        // Simulate steering angle response (simple first-order model)
        self.current_angle += clamped_torque * self.dt_s * 0.5;

        // Output: 2 × f32 = 8 bytes [steering_torque_nm, current_angle_rad]
        output.extend_from_slice(&clamped_torque.to_le_bytes());
        output.extend_from_slice(&self.current_angle.to_le_bytes());
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(v) = params.get("kp").and_then(|v| v.as_f64()) {
            self.kp = (v as f32).max(0.1).min(50.0);
        }
        if let Some(v) = params.get("ki").and_then(|v| v.as_f64()) {
            self.ki = (v as f32).max(0.0).min(10.0);
        }
        if let Some(v) = params.get("kd").and_then(|v| v.as_f64()) {
            self.kd = (v as f32).max(0.0).min(20.0);
        }
        if let Some(v) = params.get("max_torque_nm").and_then(|v| v.as_f64()) {
            self.max_torque_nm = (v as f32).max(1.0).min(20.0);
        }
        if let Some(v) = params.get("dt_s").and_then(|v| v.as_f64()) {
            self.dt_s = (v as f32).max(0.001).min(0.1);
        }
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "SteeringController".to_string(),
            version: 1,
            description: "ADAS PID steering controller — converts desired steering angle to torque command with anti-windup, running at 100 Hz.".to_string(),
            category: "ADAS".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "steer_angle_in".to_string(),
                data_type: "f32".to_string(),
                sample_size_bytes: 4,
                description: "Desired steering angle from PathPlanner (radians)".to_string(),
                example_values: "0.115".to_string(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "torque_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Steering output: [torque_nm, current_angle_rad]".to_string(),
                example_values: "1.725, 0.008625".to_string(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "kp".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(15.0),
                    description: "Proportional gain".to_string(),
                },
                ConfigParam {
                    name: "ki".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(0.5),
                    description: "Integral gain".to_string(),
                },
                ConfigParam {
                    name: "kd".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(2.0),
                    description: "Derivative gain".to_string(),
                },
                ConfigParam {
                    name: "max_torque_nm".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(5.0),
                    description: "Maximum steering torque output in Nm".to_string(),
                },
                ConfigParam {
                    name: "dt_s".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(0.01),
                    description: "Control loop period in seconds (100 Hz default)".to_string(),
                },
            ],
            scheduling_type: SchedulingType::Periodic,
            timing: TimingInfo {
                wcet_us: 200,
                bcet_us: 50,
                typical_us: 100,
            },
            resources: ResourceInfo {
                stack_size_bytes: 4096,
                static_mem_bytes: 256,
                requires_fpu: true,
                requires_gpu: false,
            },
            asil_level: AsilLevel::AsilB,
        }
    }
}

edf_core::declare_edf_module!(SteeringControllerModule);
