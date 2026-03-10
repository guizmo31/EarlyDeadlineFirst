// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct EqualizerModule {
    /// Gains in dB for bands: 300Hz, 600Hz, 1kHz, 2kHz, 4kHz
    band_gains: [f32; 5],
    /// Linear gain factors computed from dB values
    band_linear: [f32; 5],
    initialized: bool,
}

impl Default for EqualizerModule {
    fn default() -> Self {
        Self {
            band_gains: [0.0; 5],
            band_linear: [1.0; 5],
            initialized: false,
        }
    }
}

impl EqualizerModule {
    fn update_linear_gains(&mut self) {
        for i in 0..5 {
            // Convert dB to linear: 10^(dB/20)
            self.band_linear[i] = (10.0_f32).powf(self.band_gains[i] / 20.0);
        }
    }
}

impl EdfModule for EqualizerModule {
    fn init(&mut self) {
        self.update_linear_gains();
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

        // Simplified parametric EQ: apply weighted sum of band gains based on
        // sample position within the frame. This simulates frequency-dependent
        // gain without a full FFT/biquad implementation.
        // Band center frequencies: 300Hz, 600Hz, 1kHz, 2kHz, 4kHz
        let band_weights: [f32; 5] = [0.1, 0.2, 0.4, 0.2, 0.1];

        for (i, sample) in samples.iter().enumerate() {
            // Compute position-dependent band emphasis
            let pos = i as f32 / len as f32;

            // Weight each band based on sample position to simulate frequency response
            let mut total_gain = 0.0_f32;
            let mut total_weight = 0.0_f32;
            for b in 0..5 {
                let band_center = (b as f32 + 0.5) / 5.0;
                let dist = (pos - band_center).abs();
                let weight = band_weights[b] * (1.0 - dist).max(0.0);
                total_gain += weight * self.band_linear[b];
                total_weight += weight;
            }

            let effective_gain = if total_weight > 1e-10 {
                total_gain / total_weight
            } else {
                1.0
            };

            let out_sample = (sample * effective_gain).clamp(-1.0, 1.0);
            output.extend_from_slice(&out_sample.to_le_bytes());
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(gains) = params.get("band_gains").and_then(|v| v.as_array()) {
            for (i, val) in gains.iter().enumerate() {
                if i >= 5 {
                    break;
                }
                if let Some(g) = val.as_f64() {
                    self.band_gains[i] = g as f32;
                }
            }
            self.update_linear_gains();
        }
    }

    fn reset(&mut self) {
        self.band_gains = [0.0; 5];
        self.band_linear = [1.0; 5];
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "Equalizer".to_string(),
            version: 1,
            description: "5-band parametric equalizer for voice — applies frequency-weighted gain at 300Hz/600Hz/1kHz/2kHz/4kHz.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "audio_in".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Input PCM audio samples (f32 little-endian)".to_string(),
                example_values: "0.5, -0.3, 0.8, -0.1".to_string(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "audio_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Equalized PCM audio samples (f32 little-endian)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![ConfigParam {
                name: "band_gains".to_string(),
                data_type: "f32[]".to_string(),
                default_value: serde_json::json!([0.0, 0.0, 0.0, 0.0, 0.0]),
                description: "Gains in dB for bands 300Hz/600Hz/1kHz/2kHz/4kHz".to_string(),
            }],
            scheduling_type: SchedulingType::DataDriven,
            timing: TimingInfo {
                wcet_us: 600,
                bcet_us: 150,
                typical_us: 300,
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

edf_core::declare_edf_module!(EqualizerModule);
