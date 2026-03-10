// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct Limiter {
    threshold: f32,
    release_ms: f32,
    attack_ms: f32,
    envelope: f32,
    initialized: bool,
}

impl Default for Limiter {
    fn default() -> Self {
        Self {
            threshold: 0.95,
            release_ms: 10.0,
            attack_ms: 0.1,
            envelope: 0.0,
            initialized: false,
        }
    }
}

impl EdfModule for Limiter {
    fn init(&mut self) {
        self.threshold = 0.95;
        self.release_ms = 10.0;
        self.attack_ms = 0.1;
        self.envelope = 0.0;
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }
        let input = inputs[0];
        let output = &mut outputs[0];
        output.clear();

        // Assume 48000 Hz sample rate for envelope coefficients
        let sample_rate = 48000.0_f32;
        let attack_coeff = if self.attack_ms > 0.0 {
            (-1.0 / (self.attack_ms * 0.001 * sample_rate)).exp()
        } else {
            0.0
        };
        let release_coeff = if self.release_ms > 0.0 {
            (-1.0 / (self.release_ms * 0.001 * sample_rate)).exp()
        } else {
            0.0
        };

        for chunk in input.chunks_exact(4) {
            let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let abs_sample = sample.abs();

            // Track peak envelope with attack/release
            if abs_sample > self.envelope {
                self.envelope = attack_coeff * self.envelope + (1.0 - attack_coeff) * abs_sample;
            } else {
                self.envelope = release_coeff * self.envelope + (1.0 - release_coeff) * abs_sample;
            }

            // Apply gain reduction when envelope exceeds threshold
            let out_sample = if self.envelope > self.threshold {
                sample * (self.threshold / self.envelope)
            } else {
                sample
            };

            output.extend_from_slice(&out_sample.to_le_bytes());
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(v) = params.get("threshold").and_then(|v| v.as_f64()) {
            self.threshold = v as f32;
        }
        if let Some(v) = params.get("release_ms").and_then(|v| v.as_f64()) {
            self.release_ms = v as f32;
        }
        if let Some(v) = params.get("attack_ms").and_then(|v| v.as_f64()) {
            self.attack_ms = v as f32;
        }
    }

    fn reset(&mut self) {
        self.threshold = 0.95;
        self.release_ms = 10.0;
        self.attack_ms = 0.1;
        self.envelope = 0.0;
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "Limiter".to_string(),
            version: 1,
            description: "Peak limiter to prevent clipping. Tracks peak level with attack/release envelope and applies gain reduction when signal exceeds threshold.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "audio_in".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Input PCM audio samples (f32 little-endian)".to_string(),
                example_values: "0.5, -0.99, 1.2, -0.3".to_string(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "audio_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Limited PCM audio samples (f32 little-endian)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "threshold".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(0.95),
                    description: "Limiter threshold (0.0 to 1.0)".to_string(),
                },
                ConfigParam {
                    name: "release_ms".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(10.0),
                    description: "Release time in milliseconds".to_string(),
                },
                ConfigParam {
                    name: "attack_ms".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(0.1),
                    description: "Attack time in milliseconds".to_string(),
                },
            ],
            scheduling_type: SchedulingType::DataDriven,
            timing: TimingInfo {
                wcet_us: 200,
                bcet_us: 50,
                typical_us: 100,
            },
            resources: ResourceInfo {
                stack_size_bytes: 4096,
                static_mem_bytes: 1024,
                requires_fpu: true,
                requires_gpu: false,
            },
            asil_level: AsilLevel::QM,
        }
    }
}

edf_core::declare_edf_module!(Limiter);
