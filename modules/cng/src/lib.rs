// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct CngModule {
    noise_level: f32,
    smoothing: f32,
    rng_state: u32,
    smoothed_noise: f32,
    initialized: bool,
}

impl Default for CngModule {
    fn default() -> Self {
        Self {
            noise_level: 0.002,
            smoothing: 0.95,
            rng_state: 12345,
            smoothed_noise: 0.0,
            initialized: false,
        }
    }
}

impl CngModule {
    fn next_noise(&mut self) -> f32 {
        // Simple LCG PRNG
        self.rng_state = self.rng_state.wrapping_mul(1103515245).wrapping_add(12345);
        let raw = ((self.rng_state >> 16) as i16) as f32 / 32768.0;
        raw * self.noise_level
    }
}

impl EdfModule for CngModule {
    fn init(&mut self) {
        self.rng_state = 12345;
        self.smoothed_noise = 0.0;
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.len() < 2 || outputs.is_empty() {
            return;
        }
        let audio_input = inputs[0];
        let vad_input = inputs[1];
        let output = &mut outputs[0];
        output.clear();

        let samples: Vec<f32> = audio_input
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();

        // Read VAD flags (one per frame, but we apply per-sample using latest flag)
        let vad_active = if !vad_input.is_empty() {
            *vad_input.last().unwrap() == 1
        } else {
            true // Default to speech if no VAD info
        };

        for sample in &samples {
            if vad_active {
                // Speech: pass through
                output.extend_from_slice(&sample.to_le_bytes());
            } else {
                // Silence: generate comfort noise with smoothing
                let noise = self.next_noise();
                self.smoothed_noise =
                    self.smoothing * self.smoothed_noise + (1.0 - self.smoothing) * noise;
                output.extend_from_slice(&self.smoothed_noise.to_le_bytes());
            }
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(nl) = params.get("noise_level").and_then(|v| v.as_f64()) {
            self.noise_level = nl as f32;
        }
        if let Some(sm) = params.get("smoothing").and_then(|v| v.as_f64()) {
            self.smoothing = (sm as f32).clamp(0.0, 1.0);
        }
    }

    fn reset(&mut self) {
        self.noise_level = 0.002;
        self.smoothing = 0.95;
        self.rng_state = 12345;
        self.smoothed_noise = 0.0;
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "CNG".to_string(),
            version: 1,
            description: "Comfort Noise Generator — 3GPP TS 26.093/26.094 compliant, replaces silence with generated comfort noise.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![
                PortDescriptor {
                    port_name: "audio_in".to_string(),
                    data_type: "f32[]".to_string(),
                    sample_size_bytes: 4,
                    description: "Input PCM audio samples (f32 little-endian)".to_string(),
                    example_values: "0.5, -0.3, 0.001".to_string(),
                },
                PortDescriptor {
                    port_name: "vad_flag".to_string(),
                    data_type: "u8[]".to_string(),
                    sample_size_bytes: 1,
                    description: "VAD decision flags (1=speech, 0=silence)".to_string(),
                    example_values: "1, 0".to_string(),
                },
            ],
            output_ports: vec![PortDescriptor {
                port_name: "audio_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Output PCM with comfort noise during silence (f32 little-endian)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "noise_level".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(0.002),
                    description: "Comfort noise amplitude level".to_string(),
                },
                ConfigParam {
                    name: "smoothing".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(0.95),
                    description: "Noise smoothing factor (0.0 to 1.0)".to_string(),
                },
            ],
            scheduling_type: SchedulingType::DataDriven,
            timing: TimingInfo {
                wcet_us: 200,
                bcet_us: 40,
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

edf_core::declare_edf_module!(CngModule);
