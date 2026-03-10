// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct GainModule {
    gain: f32,
    initialized: bool,
}

impl Default for GainModule {
    fn default() -> Self {
        Self {
            gain: 1.0,
            initialized: false,
        }
    }
}

impl EdfModule for GainModule {
    fn init(&mut self) {
        self.gain = 1.0;
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }
        let input = inputs[0];
        let output = &mut outputs[0];
        output.clear();

        for chunk in input.chunks_exact(4) {
            let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let amplified = sample * self.gain;
            output.extend_from_slice(&amplified.to_le_bytes());
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(g) = params.get("gain").and_then(|v| v.as_f64()) {
            self.gain = g as f32;
        }
    }

    fn reset(&mut self) {
        self.gain = 1.0;
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "Gain".to_string(),
            version: 1,
            description: "Audio gain module — multiplies input PCM samples by a configurable gain factor.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "audio_in".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Raw PCM audio samples (f32 little-endian)".to_string(),
                example_values: "0.5, 1.0, -0.25, 0.75".to_string(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "audio_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Amplified PCM audio samples (f32 little-endian)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![ConfigParam {
                name: "gain".to_string(),
                data_type: "f32".to_string(),
                default_value: serde_json::json!(1.0),
                description: "Amplification factor".to_string(),
            }],
            scheduling_type: SchedulingType::Periodic,
            timing: TimingInfo {
                wcet_us: 200,
                bcet_us: 50,
                typical_us: 120,
            },
            resources: ResourceInfo {
                stack_size_bytes: 2048,
                static_mem_bytes: 512,
                requires_fpu: true,
                requires_gpu: false,
            },
            asil_level: AsilLevel::QM,
        }
    }
}

edf_core::declare_edf_module!(GainModule);
