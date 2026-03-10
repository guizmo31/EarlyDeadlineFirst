// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct BeamFormingModule {
    mic_spacing_mm: f32,
    target_angle_deg: f32,
    num_mics: usize,
    delay_samples: usize,
    initialized: bool,
}

impl Default for BeamFormingModule {
    fn default() -> Self {
        Self {
            mic_spacing_mm: 50.0,
            target_angle_deg: 0.0,
            num_mics: 2,
            delay_samples: 0,
            initialized: false,
        }
    }
}

impl BeamFormingModule {
    fn compute_delay(&mut self) {
        // Speed of sound ~343 m/s, compute inter-mic delay in samples at 48kHz
        let speed_of_sound = 343.0_f32; // m/s
        let spacing_m = self.mic_spacing_mm / 1000.0;
        let angle_rad = self.target_angle_deg.to_radians();
        let delay_s = (spacing_m * angle_rad.sin()) / speed_of_sound;
        let sample_rate = 48000.0_f32;
        self.delay_samples = (delay_s.abs() * sample_rate) as usize;
    }
}

impl EdfModule for BeamFormingModule {
    fn init(&mut self) {
        self.compute_delay();
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.len() < 2 || outputs.is_empty() {
            return;
        }
        let output = &mut outputs[0];
        output.clear();

        let mic0_samples: Vec<f32> = inputs[0]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();

        let mic1_samples: Vec<f32> = inputs[1]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();

        let len = mic0_samples.len().min(mic1_samples.len());
        let norm = self.num_mics as f32;

        for i in 0..len {
            let s0 = mic0_samples[i];
            // Apply delay compensation for mic_1
            let delayed_idx = if i >= self.delay_samples {
                i - self.delay_samples
            } else {
                0
            };
            let s1 = mic1_samples[delayed_idx];

            // Delay-and-sum with normalization
            let beam = (s0 + s1) / norm;
            output.extend_from_slice(&beam.to_le_bytes());
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(ms) = params.get("mic_spacing_mm").and_then(|v| v.as_f64()) {
            self.mic_spacing_mm = ms as f32;
        }
        if let Some(ta) = params.get("target_angle_deg").and_then(|v| v.as_f64()) {
            self.target_angle_deg = ta as f32;
        }
        if let Some(nm) = params.get("num_mics").and_then(|v| v.as_u64()) {
            self.num_mics = (nm as usize).max(1);
        }
        self.compute_delay();
    }

    fn reset(&mut self) {
        self.mic_spacing_mm = 50.0;
        self.target_angle_deg = 0.0;
        self.num_mics = 2;
        self.delay_samples = 0;
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "BeamForming".to_string(),
            version: 1,
            description: "Multi-microphone beamforming — delay-and-sum algorithm for spatial audio filtering.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![
                PortDescriptor {
                    port_name: "mic_0".to_string(),
                    data_type: "f32[]".to_string(),
                    sample_size_bytes: 4,
                    description: "First microphone PCM samples (f32 little-endian)".to_string(),
                    example_values: "0.5, -0.3, 0.1".to_string(),
                },
                PortDescriptor {
                    port_name: "mic_1".to_string(),
                    data_type: "f32[]".to_string(),
                    sample_size_bytes: 4,
                    description: "Second microphone PCM samples (f32 little-endian)".to_string(),
                    example_values: "0.4, -0.2, 0.05".to_string(),
                },
            ],
            output_ports: vec![PortDescriptor {
                port_name: "beam_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Beamformed PCM audio output (f32 little-endian)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "mic_spacing_mm".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(50.0),
                    description: "Inter-microphone spacing in millimeters".to_string(),
                },
                ConfigParam {
                    name: "target_angle_deg".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(0.0),
                    description: "Target steering angle in degrees".to_string(),
                },
                ConfigParam {
                    name: "num_mics".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(2),
                    description: "Number of microphones in array".to_string(),
                },
            ],
            scheduling_type: SchedulingType::DataDriven,
            timing: TimingInfo {
                wcet_us: 1000,
                bcet_us: 200,
                typical_us: 500,
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

edf_core::declare_edf_module!(BeamFormingModule);
