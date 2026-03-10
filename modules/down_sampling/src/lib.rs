// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct DownSamplingModule {
    factor: usize,
    initialized: bool,
}

impl Default for DownSamplingModule {
    fn default() -> Self {
        Self {
            factor: 2,
            initialized: false,
        }
    }
}

impl EdfModule for DownSamplingModule {
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

        for chunk in samples.chunks(self.factor) {
            let sum: f32 = chunk.iter().sum();
            let avg = sum / chunk.len() as f32;
            output.extend_from_slice(&avg.to_le_bytes());
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(f) = params.get("factor").and_then(|v| v.as_u64()) {
            self.factor = (f as usize).max(1);
        }
    }

    fn reset(&mut self) {
        self.factor = 2;
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "DownSampling".to_string(),
            version: 1,
            description: "Audio down-sampling module — decimates signal by an integer factor with averaging anti-alias filter.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "audio_in".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "PCM audio samples at original rate (f32 little-endian)".to_string(),
                example_values: "1.0, 2.0, 3.0, 4.0, 5.0, 6.0".to_string(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "audio_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Decimated PCM audio samples (f32 little-endian)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![ConfigParam {
                name: "factor".to_string(),
                data_type: "usize".to_string(),
                default_value: serde_json::json!(2),
                description: "Decimation factor".to_string(),
            }],
            scheduling_type: SchedulingType::Periodic,
            timing: TimingInfo {
                wcet_us: 300,
                bcet_us: 80,
                typical_us: 180,
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

edf_core::declare_edf_module!(DownSamplingModule);
