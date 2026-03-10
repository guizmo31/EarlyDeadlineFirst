// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

/// LKA — EPS (Electric Power Steering) actuator interface module.
///
/// Converts a steering torque command into a PWM duty cycle and direction
/// signal for the EPS motor driver. Applies rate limiting and safety checks.
pub struct SteeringActuatorModule {
    max_torque_nm: f32,
    rate_limit_nm_per_s: f32,
    prev_torque: f32,
    dt_s: f32,
    fault_active: bool,
    initialized: bool,
}

impl Default for SteeringActuatorModule {
    fn default() -> Self {
        Self {
            max_torque_nm: 5.0,
            rate_limit_nm_per_s: 50.0,
            prev_torque: 0.0,
            dt_s: 0.01,
            fault_active: false,
            initialized: false,
        }
    }
}

impl EdfModule for SteeringActuatorModule {
    fn init(&mut self) {
        self.prev_torque = 0.0;
        self.fault_active = false;
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }
        let torque_data = inputs[0];
        let output = &mut outputs[0];
        output.clear();

        // Expect 2 × f32 = 8 bytes from SteeringController [torque_nm, current_angle_rad]
        if torque_data.len() < 8 {
            // Safety: output zero torque on invalid input
            output.extend_from_slice(&0.0f32.to_le_bytes()); // pwm_duty
            output.push(0u8); // direction: 0=neutral
            output.push(0u8); // fault_flag
            return;
        }

        let requested_torque = f32::from_le_bytes([
            torque_data[0], torque_data[1], torque_data[2], torque_data[3],
        ]);

        // Rate limiting: prevent sudden torque changes
        let max_delta = self.rate_limit_nm_per_s * self.dt_s;
        let delta = (requested_torque - self.prev_torque).clamp(-max_delta, max_delta);
        let limited_torque = self.prev_torque + delta;

        // Clamp to actuator physical limits
        let clamped_torque = limited_torque.clamp(-self.max_torque_nm, self.max_torque_nm);

        // Convert to PWM duty cycle (0.0–1.0)
        let pwm_duty = clamped_torque.abs() / self.max_torque_nm;

        // Direction: 1 = steer right, 2 = steer left, 0 = neutral
        let direction: u8 = if clamped_torque.abs() < 0.01 {
            0 // neutral / deadband
        } else if clamped_torque > 0.0 {
            1 // right
        } else {
            2 // left
        };

        // Fault detection: if torque demand changes sign too rapidly
        let fault_flag: u8 = if self.prev_torque.signum() != clamped_torque.signum()
            && self.prev_torque.abs() > self.max_torque_nm * 0.5
        {
            self.fault_active = true;
            1
        } else {
            0
        };

        self.prev_torque = clamped_torque;

        // Output: f32 (pwm_duty) + u8 (direction) + u8 (fault_flag) = 6 bytes
        output.extend_from_slice(&pwm_duty.to_le_bytes());
        output.push(direction);
        output.push(fault_flag);
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(v) = params.get("max_torque_nm").and_then(|v| v.as_f64()) {
            self.max_torque_nm = (v as f32).max(1.0).min(20.0);
        }
        if let Some(v) = params.get("rate_limit_nm_per_s").and_then(|v| v.as_f64()) {
            self.rate_limit_nm_per_s = (v as f32).max(10.0).min(200.0);
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
            name: "SteeringActuator".to_string(),
            version: 1,
            description: "ADAS EPS actuator interface — converts steering torque to PWM duty cycle with rate limiting and fault detection for safe LKA actuation.".to_string(),
            category: "ADAS".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "torque_in".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Steering output from SteeringController: [torque_nm, current_angle_rad]".to_string(),
                example_values: "1.725, 0.008625".to_string(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "actuator_cmd".to_string(),
                data_type: "mixed".to_string(),
                sample_size_bytes: 6,
                description: "EPS actuator command: f32 PWM duty (0–1) + u8 direction (0=neutral, 1=right, 2=left) + u8 fault flag".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "max_torque_nm".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(5.0),
                    description: "Maximum actuator torque in Nm".to_string(),
                },
                ConfigParam {
                    name: "rate_limit_nm_per_s".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(50.0),
                    description: "Maximum torque rate of change in Nm/s".to_string(),
                },
                ConfigParam {
                    name: "dt_s".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(0.01),
                    description: "Control loop period in seconds".to_string(),
                },
            ],
            scheduling_type: SchedulingType::Periodic,
            timing: TimingInfo {
                wcet_us: 150,
                bcet_us: 30,
                typical_us: 80,
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

edf_core::declare_edf_module!(SteeringActuatorModule);
