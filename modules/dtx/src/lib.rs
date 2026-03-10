// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct DTX {
    sid_interval_frames: usize,
    sid_frame_size: usize,
    silence_frame_counter: usize,
    initialized: bool,
}

impl Default for DTX {
    fn default() -> Self {
        Self {
            sid_interval_frames: 8,
            sid_frame_size: 5,
            silence_frame_counter: 0,
            initialized: false,
        }
    }
}

impl EdfModule for DTX {
    fn init(&mut self) {
        self.sid_interval_frames = 8;
        self.sid_frame_size = 5;
        self.silence_frame_counter = 0;
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.len() < 2 || outputs.is_empty() {
            return;
        }
        let audio_in = inputs[0];
        let vad_flag = inputs[1];
        let output = &mut outputs[0];
        output.clear();

        // VAD flag: non-empty and first byte != 0 means voice active
        let voice_active = !vad_flag.is_empty() && vad_flag[0] != 0;

        if voice_active {
            // Pass encoded frame through
            self.silence_frame_counter = 0;
            output.extend_from_slice(audio_in);
        } else {
            // Silence: output SID frame at sid_interval
            self.silence_frame_counter += 1;
            if self.silence_frame_counter >= self.sid_interval_frames {
                self.silence_frame_counter = 0;
                // Generate SID (Silence Insertion Descriptor) frame
                // SID frame: marker byte 0xFF followed by zero-filled payload
                output.push(0xFF);
                for _ in 1..self.sid_frame_size {
                    output.push(0x00);
                }
            }
            // Otherwise output nothing (DTX active, no transmission)
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(v) = params.get("sid_interval_frames").and_then(|v| v.as_u64()) {
            self.sid_interval_frames = v as usize;
        }
        if let Some(v) = params.get("sid_frame_size").and_then(|v| v.as_u64()) {
            self.sid_frame_size = v as usize;
        }
    }

    fn reset(&mut self) {
        self.sid_interval_frames = 8;
        self.sid_frame_size = 5;
        self.silence_frame_counter = 0;
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "DTX".to_string(),
            version: 1,
            description: "3GPP TS 26.093 Discontinuous Transmission. Passes voice frames through when VAD=1, outputs SID frames at configurable intervals during silence.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![
                PortDescriptor {
                    port_name: "audio_in".to_string(),
                    data_type: "u8[]".to_string(),
                    sample_size_bytes: 1,
                    description: "Encoded audio frames".to_string(),
                    example_values: "0x07, 0x1A, ...".to_string(),
                },
                PortDescriptor {
                    port_name: "vad_flag".to_string(),
                    data_type: "u8[]".to_string(),
                    sample_size_bytes: 1,
                    description: "Voice Activity Detection flag (0 = silence, 1 = voice)".to_string(),
                    example_values: "0x01".to_string(),
                },
            ],
            output_ports: vec![PortDescriptor {
                port_name: "tx_out".to_string(),
                data_type: "u8[]".to_string(),
                sample_size_bytes: 1,
                description: "Transmitted frames (voice frames or SID frames)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "sid_interval_frames".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(8),
                    description: "Number of silence frames between SID updates".to_string(),
                },
                ConfigParam {
                    name: "sid_frame_size".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(5),
                    description: "Size of SID frame in bytes".to_string(),
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
                static_mem_bytes: 2048,
                requires_fpu: false,
                requires_gpu: false,
            },
            asil_level: AsilLevel::QM,
        }
    }
}

edf_core::declare_edf_module!(DTX);
