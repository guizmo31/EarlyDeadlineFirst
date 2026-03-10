// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct VadModule {
    energy_threshold: f32,
    hangover_frames: usize,
    frame_size: usize,
    hangover_counter: usize,
    speech_active: bool,
    initialized: bool,
}

impl Default for VadModule {
    fn default() -> Self {
        Self {
            energy_threshold: 0.005,
            hangover_frames: 5,
            frame_size: 160,
            hangover_counter: 0,
            speech_active: false,
            initialized: false,
        }
    }
}

impl EdfModule for VadModule {
    fn init(&mut self) {
        self.hangover_counter = 0;
        self.speech_active = false;
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.is_empty() || outputs.len() < 2 {
            return;
        }
        let input = inputs[0];
        let (first, rest) = outputs.split_at_mut(1);
        let audio_out = &mut first[0];
        let vad_out = &mut rest[0];
        audio_out.clear();
        vad_out.clear();

        let samples: Vec<f32> = input
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();

        // Process in frames
        let mut offset = 0;
        while offset + self.frame_size <= samples.len() {
            let frame = &samples[offset..offset + self.frame_size];

            // Compute frame energy
            let energy: f32 = frame.iter().map(|s| s * s).sum::<f32>() / self.frame_size as f32;

            // VAD decision with hangover
            if energy > self.energy_threshold {
                self.speech_active = true;
                self.hangover_counter = self.hangover_frames;
            } else if self.hangover_counter > 0 {
                self.hangover_counter -= 1;
                self.speech_active = true;
            } else {
                self.speech_active = false;
            }

            let vad_flag: u8 = if self.speech_active { 1 } else { 0 };

            // Pass through audio samples
            for sample in frame {
                audio_out.extend_from_slice(&sample.to_le_bytes());
            }

            // Output VAD flag for this frame
            vad_out.push(vad_flag);

            offset += self.frame_size;
        }

        // Handle remaining samples (partial frame treated as speech if active)
        if offset < samples.len() {
            let vad_flag: u8 = if self.speech_active { 1 } else { 0 };
            for sample in &samples[offset..] {
                audio_out.extend_from_slice(&sample.to_le_bytes());
            }
            vad_out.push(vad_flag);
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(et) = params.get("energy_threshold").and_then(|v| v.as_f64()) {
            self.energy_threshold = et as f32;
        }
        if let Some(hf) = params.get("hangover_frames").and_then(|v| v.as_u64()) {
            self.hangover_frames = hf as usize;
        }
        if let Some(fs) = params.get("frame_size").and_then(|v| v.as_u64()) {
            self.frame_size = (fs as usize).max(1);
        }
    }

    fn reset(&mut self) {
        self.energy_threshold = 0.005;
        self.hangover_frames = 5;
        self.frame_size = 160;
        self.hangover_counter = 0;
        self.speech_active = false;
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "VAD".to_string(),
            version: 1,
            description: "Voice Activity Detection — 3GPP TS 26.092 energy-based VAD with hangover logic.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "audio_in".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Input PCM audio samples (f32 little-endian)".to_string(),
                example_values: "0.5, -0.3, 0.001, 0.002".to_string(),
            }],
            output_ports: vec![
                PortDescriptor {
                    port_name: "audio_out".to_string(),
                    data_type: "f32[]".to_string(),
                    sample_size_bytes: 4,
                    description: "Passthrough PCM audio samples (f32 little-endian)".to_string(),
                    example_values: String::new(),
                },
                PortDescriptor {
                    port_name: "vad_flag".to_string(),
                    data_type: "u8[]".to_string(),
                    sample_size_bytes: 1,
                    description: "VAD decision per frame (1=speech, 0=silence)".to_string(),
                    example_values: "1, 0, 1".to_string(),
                },
            ],
            config_params: vec![
                ConfigParam {
                    name: "energy_threshold".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(0.005),
                    description: "Energy threshold for speech detection".to_string(),
                },
                ConfigParam {
                    name: "hangover_frames".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(5),
                    description: "Number of hangover frames after speech ends".to_string(),
                },
                ConfigParam {
                    name: "frame_size".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(160),
                    description: "Frame size in samples".to_string(),
                },
            ],
            scheduling_type: SchedulingType::DataDriven,
            timing: TimingInfo {
                wcet_us: 300,
                bcet_us: 50,
                typical_us: 150,
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

edf_core::declare_edf_module!(VadModule);
