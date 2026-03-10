// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct AgcModule {
    target_level: f32,
    attack_ms: f32,
    release_ms: f32,
    max_gain: f32,
    current_gain: f32,
    initialized: bool,
}

impl Default for AgcModule {
    fn default() -> Self {
        Self {
            target_level: 0.5,
            attack_ms: 5.0,
            release_ms: 50.0,
            max_gain: 30.0,
            current_gain: 1.0,
            initialized: false,
        }
    }
}

impl EdfModule for AgcModule {
    fn init(&mut self) {
        self.current_gain = 1.0;
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }
        let input = inputs[0];
        let output = &mut outputs[0];
        output.clear();

        let samples: Vec<f32> = input
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();

        if samples.is_empty() {
            return;
        }

        // Compute RMS level
        let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
        let rms = (sum_sq / samples.len() as f32).sqrt();

        // Compute desired gain
        let desired_gain = if rms > 1e-10 {
            (self.target_level / rms).min(self.max_gain)
        } else {
            self.current_gain
        };

        // Smooth gain adjustment with attack/release
        let sample_rate = 48000.0_f32;
        let alpha = if desired_gain < self.current_gain {
            // Attack (reducing gain = fast response)
            1.0 - (-1.0 / (self.attack_ms * 0.001 * sample_rate)).exp()
        } else {
            // Release (increasing gain = slow response)
            1.0 - (-1.0 / (self.release_ms * 0.001 * sample_rate)).exp()
        };

        self.current_gain += alpha * (desired_gain - self.current_gain);

        // Apply gain
        for sample in &samples {
            let amplified = (sample * self.current_gain).clamp(-1.0, 1.0);
            output.extend_from_slice(&amplified.to_le_bytes());
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(tl) = params.get("target_level").and_then(|v| v.as_f64()) {
            self.target_level = (tl as f32).max(0.0);
        }
        if let Some(a) = params.get("attack_ms").and_then(|v| v.as_f64()) {
            self.attack_ms = (a as f32).max(0.1);
        }
        if let Some(r) = params.get("release_ms").and_then(|v| v.as_f64()) {
            self.release_ms = (r as f32).max(0.1);
        }
        if let Some(mg) = params.get("max_gain").and_then(|v| v.as_f64()) {
            self.max_gain = (mg as f32).max(1.0);
        }
    }

    fn reset(&mut self) {
        self.target_level = 0.5;
        self.attack_ms = 5.0;
        self.release_ms = 50.0;
        self.max_gain = 30.0;
        self.current_gain = 1.0;
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "AGC".to_string(),
            version: 1,
            description: "Automatic Gain Control — measures RMS level and adjusts gain toward target with attack/release envelope.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "audio_in".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Input PCM audio samples (f32 little-endian)".to_string(),
                example_values: "0.1, -0.05, 0.2, -0.15".to_string(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "audio_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Gain-adjusted PCM audio samples (f32 little-endian)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "target_level".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(0.5),
                    description: "Target RMS output level".to_string(),
                },
                ConfigParam {
                    name: "attack_ms".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(5.0),
                    description: "Attack time in milliseconds".to_string(),
                },
                ConfigParam {
                    name: "release_ms".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(50.0),
                    description: "Release time in milliseconds".to_string(),
                },
                ConfigParam {
                    name: "max_gain".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(30.0),
                    description: "Maximum gain factor".to_string(),
                },
            ],
            scheduling_type: SchedulingType::DataDriven,
            timing: TimingInfo {
                wcet_us: 400,
                bcet_us: 100,
                typical_us: 200,
            },
            resources: ResourceInfo {
                stack_size_bytes: 4096,
                static_mem_bytes: 2048,
                requires_fpu: true,
                requires_gpu: false,
            },
            asil_level: AsilLevel::QM,
        }
    }
}

edf_core::declare_edf_module!(AgcModule);
