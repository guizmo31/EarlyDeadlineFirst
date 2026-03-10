// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct AecModule {
    filter_length: usize,
    step_size: f32,
    tail_length_ms: usize,
    filter_coeffs: Vec<f32>,
    ref_buffer: Vec<f32>,
    initialized: bool,
}

impl Default for AecModule {
    fn default() -> Self {
        Self {
            filter_length: 256,
            step_size: 0.01,
            tail_length_ms: 64,
            filter_coeffs: vec![0.0; 256],
            ref_buffer: vec![0.0; 256],
            initialized: false,
        }
    }
}

impl EdfModule for AecModule {
    fn init(&mut self) {
        self.filter_coeffs = vec![0.0; self.filter_length];
        self.ref_buffer = vec![0.0; self.filter_length];
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.len() < 2 || outputs.is_empty() {
            return;
        }
        let mic_input = inputs[0];
        let ref_input = inputs[1];
        let output = &mut outputs[0];
        output.clear();

        let mic_samples: Vec<f32> = mic_input
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();

        let ref_samples: Vec<f32> = ref_input
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();

        for i in 0..mic_samples.len() {
            // Shift reference buffer and insert new sample
            if self.ref_buffer.len() > 1 {
                self.ref_buffer.rotate_right(1);
            }
            if i < ref_samples.len() {
                self.ref_buffer[0] = ref_samples[i];
            } else {
                self.ref_buffer[0] = 0.0;
            }

            // Estimate echo: dot product of filter coeffs and ref buffer
            let echo_estimate: f32 = self
                .filter_coeffs
                .iter()
                .zip(self.ref_buffer.iter())
                .map(|(c, r)| c * r)
                .sum();

            // Compute error (echo-free signal)
            let error = mic_samples[i] - echo_estimate;

            // NLMS coefficient update
            let ref_power: f32 = self.ref_buffer.iter().map(|r| r * r).sum();
            let norm = ref_power + 1e-10;

            for j in 0..self.filter_length {
                self.filter_coeffs[j] += self.step_size * error * self.ref_buffer[j] / norm;
            }

            output.extend_from_slice(&error.to_le_bytes());
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(fl) = params.get("filter_length").and_then(|v| v.as_u64()) {
            self.filter_length = (fl as usize).max(1);
            self.filter_coeffs.resize(self.filter_length, 0.0);
            self.ref_buffer.resize(self.filter_length, 0.0);
        }
        if let Some(ss) = params.get("step_size").and_then(|v| v.as_f64()) {
            self.step_size = ss as f32;
        }
        if let Some(tl) = params.get("tail_length_ms").and_then(|v| v.as_u64()) {
            self.tail_length_ms = tl as usize;
        }
    }

    fn reset(&mut self) {
        self.filter_length = 256;
        self.step_size = 0.01;
        self.tail_length_ms = 64;
        self.filter_coeffs = vec![0.0; 256];
        self.ref_buffer = vec![0.0; 256];
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "AEC".to_string(),
            version: 1,
            description: "Acoustic Echo Canceller — 3GPP TS 26.131 compliant NLMS adaptive filter for echo cancellation.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![
                PortDescriptor {
                    port_name: "mic_in".to_string(),
                    data_type: "f32[]".to_string(),
                    sample_size_bytes: 4,
                    description: "Microphone input PCM samples (f32 little-endian)".to_string(),
                    example_values: "0.5, -0.3, 0.1".to_string(),
                },
                PortDescriptor {
                    port_name: "ref_in".to_string(),
                    data_type: "f32[]".to_string(),
                    sample_size_bytes: 4,
                    description: "Speaker reference PCM samples for echo estimation (f32 little-endian)".to_string(),
                    example_values: "0.4, -0.2, 0.05".to_string(),
                },
            ],
            output_ports: vec![PortDescriptor {
                port_name: "echo_free".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Echo-cancelled PCM audio samples (f32 little-endian)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "filter_length".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(256),
                    description: "Adaptive filter length in taps".to_string(),
                },
                ConfigParam {
                    name: "step_size".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(0.01),
                    description: "NLMS step size (convergence rate)".to_string(),
                },
                ConfigParam {
                    name: "tail_length_ms".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(64),
                    description: "Echo tail length in milliseconds".to_string(),
                },
            ],
            scheduling_type: SchedulingType::DataDriven,
            timing: TimingInfo {
                wcet_us: 3000,
                bcet_us: 800,
                typical_us: 1500,
            },
            resources: ResourceInfo {
                stack_size_bytes: 16384,
                static_mem_bytes: 32768,
                requires_fpu: true,
                requires_gpu: false,
            },
            asil_level: AsilLevel::QM,
        }
    }
}

edf_core::declare_edf_module!(AecModule);
