// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct Mixer {
    gains: [f32; 4],
    initialized: bool,
}

impl Default for Mixer {
    fn default() -> Self {
        Self {
            gains: [1.0; 4],
            initialized: false,
        }
    }
}

impl EdfModule for Mixer {
    fn init(&mut self) {
        self.gains = [1.0; 4];
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if outputs.is_empty() {
            return;
        }
        let output = &mut outputs[0];
        output.clear();

        // Parse each input channel into f32 samples
        let mut channels: Vec<Vec<f32>> = Vec::new();
        let mut max_len: usize = 0;

        for (i, inp) in inputs.iter().enumerate() {
            if i >= 4 || inp.is_empty() {
                continue;
            }
            let samples: Vec<f32> = inp
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            if samples.len() > max_len {
                max_len = samples.len();
            }
            channels.push(
                samples
                    .iter()
                    .map(|v| *v * self.gains[i])
                    .collect(),
            );
        }

        if max_len == 0 {
            return;
        }

        // Mix: sum all channels sample-by-sample
        for j in 0..max_len {
            let mut sum: f32 = 0.0;
            for ch in &channels {
                if j < ch.len() {
                    sum += ch[j];
                }
            }

            // Normalize if clipping would occur
            if sum > 1.0 {
                sum = 1.0;
            } else if sum < -1.0 {
                sum = -1.0;
            }

            output.extend_from_slice(&sum.to_le_bytes());
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(arr) = params.get("gains").and_then(|v| v.as_array()) {
            for (i, val) in arr.iter().enumerate() {
                if i >= 4 {
                    break;
                }
                if let Some(g) = val.as_f64() {
                    self.gains[i] = g as f32;
                }
            }
        }
    }

    fn reset(&mut self) {
        self.gains = [1.0; 4];
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "Mixer".to_string(),
            version: 1,
            description: "Mixes up to 4 audio inputs with per-channel gain. Sums samples and clamps to prevent clipping.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![
                PortDescriptor {
                    port_name: "in_0".to_string(),
                    data_type: "f32[]".to_string(),
                    sample_size_bytes: 4,
                    description: "Audio input channel 0 (f32 little-endian)".to_string(),
                    example_values: "0.5, -0.3".to_string(),
                },
                PortDescriptor {
                    port_name: "in_1".to_string(),
                    data_type: "f32[]".to_string(),
                    sample_size_bytes: 4,
                    description: "Audio input channel 1 (f32 little-endian)".to_string(),
                    example_values: "0.2, 0.1".to_string(),
                },
                PortDescriptor {
                    port_name: "in_2".to_string(),
                    data_type: "f32[]".to_string(),
                    sample_size_bytes: 4,
                    description: "Audio input channel 2 (f32 little-endian)".to_string(),
                    example_values: String::new(),
                },
                PortDescriptor {
                    port_name: "in_3".to_string(),
                    data_type: "f32[]".to_string(),
                    sample_size_bytes: 4,
                    description: "Audio input channel 3 (f32 little-endian)".to_string(),
                    example_values: String::new(),
                },
            ],
            output_ports: vec![PortDescriptor {
                port_name: "mix_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Mixed audio output (f32 little-endian)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![ConfigParam {
                name: "gains".to_string(),
                data_type: "f32[]".to_string(),
                default_value: serde_json::json!([1.0, 1.0, 1.0, 1.0]),
                description: "Per-channel gain factors".to_string(),
            }],
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

edf_core::declare_edf_module!(Mixer);
