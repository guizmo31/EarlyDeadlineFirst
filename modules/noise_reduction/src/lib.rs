// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct NoiseReductionModule {
    threshold: f32,
    window_size: usize,
    initialized: bool,
}

impl Default for NoiseReductionModule {
    fn default() -> Self {
        Self {
            threshold: 0.01,
            window_size: 5,
            initialized: false,
        }
    }
}

impl EdfModule for NoiseReductionModule {
    fn init(&mut self) {
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

        let len = samples.len();
        if len == 0 {
            return;
        }

        let half = self.window_size / 2;
        for i in 0..len {
            let start = i.saturating_sub(half);
            let end = (i + half + 1).min(len);
            let sum: f32 = samples[start..end].iter().sum();
            let avg = sum / (end - start) as f32;

            let out_sample = if avg.abs() < self.threshold { 0.0 } else { avg };
            output.extend_from_slice(&out_sample.to_le_bytes());
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(t) = params.get("threshold").and_then(|v| v.as_f64()) {
            self.threshold = t as f32;
        }
        if let Some(w) = params.get("window_size").and_then(|v| v.as_u64()) {
            self.window_size = (w as usize).max(1);
        }
    }

    fn reset(&mut self) {
        self.threshold = 0.01;
        self.window_size = 5;
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "NoiseReduction".to_string(),
            version: 1,
            description: "Noise reduction module — moving-average smoothing with noise gate for audio signals.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "audio_in".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Raw PCM audio samples (f32 little-endian)".to_string(),
                example_values: "0.005, 0.003, 0.5, 0.8, 0.002, 0.7".to_string(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "audio_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Denoised PCM audio samples (f32 little-endian)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "threshold".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(0.01),
                    description: "Noise gate level".to_string(),
                },
                ConfigParam {
                    name: "window_size".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(5),
                    description: "Smoothing window length".to_string(),
                },
            ],
            scheduling_type: SchedulingType::Periodic,
            timing: TimingInfo {
                wcet_us: 800,
                bcet_us: 200,
                typical_us: 500,
            },
            resources: ResourceInfo {
                stack_size_bytes: 8192,
                static_mem_bytes: 2048,
                requires_fpu: true,
                requires_gpu: false,
            },
            asil_level: AsilLevel::QM,
        }
    }
}

edf_core::declare_edf_module!(NoiseReductionModule);
