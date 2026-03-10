// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct SpeakerOutputModule {
    sample_rate: usize,
    volume: f32,
    last_sample: f32,
    initialized: bool,
}

impl Default for SpeakerOutputModule {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            volume: 0.8,
            last_sample: 0.0,
            initialized: false,
        }
    }
}

impl EdfModule for SpeakerOutputModule {
    fn init(&mut self) {
        self.last_sample = 0.0;
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], _outputs: &mut [Vec<u8>]) {
        if inputs.is_empty() {
            return;
        }
        let input = inputs[0];

        // Apply volume scaling and track last sample for monitoring
        for chunk in input.chunks_exact(4) {
            let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let scaled = sample * self.volume;
            self.last_sample = scaled;
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(sr) = params.get("sample_rate").and_then(|v| v.as_u64()) {
            self.sample_rate = sr as usize;
        }
        if let Some(vol) = params.get("volume").and_then(|v| v.as_f64()) {
            self.volume = (vol as f32).clamp(0.0, 1.0);
        }
    }

    fn reset(&mut self) {
        self.sample_rate = 48000;
        self.volume = 0.8;
        self.last_sample = 0.0;
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "SpeakerOutput".to_string(),
            version: 1,
            description: "Simulates DAC speaker output — applies volume scaling to PCM audio samples.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "audio_in".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "PCM audio samples to play (f32 little-endian)".to_string(),
                example_values: "0.5, -0.3, 0.8, -0.1".to_string(),
            }],
            output_ports: vec![],
            config_params: vec![
                ConfigParam {
                    name: "sample_rate".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(48000),
                    description: "DAC sample rate in Hz".to_string(),
                },
                ConfigParam {
                    name: "volume".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(0.8),
                    description: "Output volume level (0.0 to 1.0)".to_string(),
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
                static_mem_bytes: 4096,
                requires_fpu: true,
                requires_gpu: false,
            },
            asil_level: AsilLevel::QM,
        }
    }
}

edf_core::declare_edf_module!(SpeakerOutputModule);
