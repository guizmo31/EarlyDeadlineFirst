// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct SampleRateConverter {
    input_rate: usize,
    output_rate: usize,
    initialized: bool,
}

impl Default for SampleRateConverter {
    fn default() -> Self {
        Self {
            input_rate: 48000,
            output_rate: 16000,
            initialized: false,
        }
    }
}

impl EdfModule for SampleRateConverter {
    fn init(&mut self) {
        self.input_rate = 48000;
        self.output_rate = 16000;
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }
        let input = inputs[0];
        let output = &mut outputs[0];
        output.clear();

        // Parse f32 samples from input
        let in_samples: Vec<f32> = input
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();

        if in_samples.is_empty() || self.input_rate == 0 || self.output_rate == 0 {
            return;
        }

        let ratio = self.output_rate as f64 / self.input_rate as f64;
        let out_len = (in_samples.len() as f64 * ratio) as usize;

        for i in 0..out_len {
            let src_pos = i as f64 / ratio;
            let idx = src_pos as usize;

            let sample = if self.output_rate < self.input_rate {
                // Downsampling: linear interpolation
                let frac = src_pos - idx as f64;
                let s0 = in_samples[idx.min(in_samples.len() - 1)];
                let s1 = in_samples[(idx + 1).min(in_samples.len() - 1)];
                s0 + (s1 - s0) * frac as f32
            } else {
                // Upsampling: zero-order hold
                in_samples[idx.min(in_samples.len() - 1)]
            };

            output.extend_from_slice(&sample.to_le_bytes());
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(v) = params.get("input_rate").and_then(|v| v.as_u64()) {
            self.input_rate = v as usize;
        }
        if let Some(v) = params.get("output_rate").and_then(|v| v.as_u64()) {
            self.output_rate = v as usize;
        }
    }

    fn reset(&mut self) {
        self.input_rate = 48000;
        self.output_rate = 16000;
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "SampleRateConverter".to_string(),
            version: 1,
            description: "Converts between sample rates (8000/16000/48000 Hz) using linear interpolation for downsampling and zero-order hold for upsampling.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "audio_in".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Input PCM audio samples (f32 little-endian)".to_string(),
                example_values: "0.5, -0.3, 0.8".to_string(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "audio_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Resampled PCM audio samples (f32 little-endian)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "input_rate".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(48000),
                    description: "Input sample rate in Hz".to_string(),
                },
                ConfigParam {
                    name: "output_rate".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(16000),
                    description: "Output sample rate in Hz".to_string(),
                },
            ],
            scheduling_type: SchedulingType::DataDriven,
            timing: TimingInfo {
                wcet_us: 800,
                bcet_us: 200,
                typical_us: 400,
            },
            resources: ResourceInfo {
                stack_size_bytes: 8192,
                static_mem_bytes: 8192,
                requires_fpu: true,
                requires_gpu: false,
            },
            asil_level: AsilLevel::QM,
        }
    }
}

edf_core::declare_edf_module!(SampleRateConverter);
