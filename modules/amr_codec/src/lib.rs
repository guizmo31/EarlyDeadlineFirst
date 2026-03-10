// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct AMRCodec {
    encode_mode: bool,
    bitrate: usize,
    wideband: bool,
    initialized: bool,
}

impl Default for AMRCodec {
    fn default() -> Self {
        Self {
            encode_mode: true,
            bitrate: 12200,
            wideband: false,
            initialized: false,
        }
    }
}

impl EdfModule for AMRCodec {
    fn init(&mut self) {
        self.encode_mode = true;
        self.bitrate = 12200;
        self.wideband = false;
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }
        let input = inputs[0];
        let output = &mut outputs[0];
        output.clear();

        if self.encode_mode {
            // Encode: quantize f32 samples to u8, prepend 2-byte frame header
            let samples: Vec<u8> = input
                .chunks_exact(4)
                .map(|c| {
                    let sample = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                    // Quantize f32 [-1.0, 1.0] to u8 [0, 255]
                    ((sample * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8
                })
                .collect();

            if samples.is_empty() {
                return;
            }

            // Frame header: byte 0 = bitrate mode index, byte 1 = frame length
            let mode_index = match self.bitrate {
                4750 => 0u8,
                5150 => 1,
                5900 => 2,
                6700 => 3,
                7400 => 4,
                7950 => 5,
                10200 => 6,
                12200 => 7,
                _ => 7,
            };
            let frame_len = samples.len().min(255) as u8;
            output.push(mode_index);
            output.push(frame_len);
            output.extend_from_slice(&samples[..frame_len as usize]);
        } else {
            // Decode: strip 2-byte header, convert u8 back to f32
            if input.len() < 2 {
                return;
            }
            let _mode_index = input[0];
            let frame_len = input[1] as usize;
            let payload = &input[2..];

            let len = frame_len.min(payload.len());
            for i in 0..len {
                let sample = (payload[i] as f32 / 255.0 - 0.5) * 2.0;
                output.extend_from_slice(&sample.to_le_bytes());
            }
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(m) = params.get("mode").and_then(|v| v.as_str()) {
            self.encode_mode = m != "decode";
        }
        if let Some(v) = params.get("bitrate").and_then(|v| v.as_u64()) {
            self.bitrate = v as usize;
        }
        if let Some(v) = params.get("wideband").and_then(|v| v.as_bool()) {
            self.wideband = v;
        }
    }

    fn reset(&mut self) {
        self.encode_mode = true;
        self.bitrate = 12200;
        self.wideband = false;
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "AMRCodec".to_string(),
            version: 1,
            description: "3GPP TS 26.071 AMR-NB/WB codec (simplified simulation). Encodes f32 PCM to compressed frames or decodes back.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "audio_in".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Raw PCM audio samples for encoding, or compressed frames for decoding".to_string(),
                example_values: "0.5, -0.3, 0.8".to_string(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "encoded_out".to_string(),
                data_type: "u8[]".to_string(),
                sample_size_bytes: 1,
                description: "Compressed AMR frames (encode) or decoded PCM samples (decode)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "mode".to_string(),
                    data_type: "string".to_string(),
                    default_value: serde_json::json!("encode"),
                    description: "Codec mode: encode or decode".to_string(),
                },
                ConfigParam {
                    name: "bitrate".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(12200),
                    description: "AMR bitrate in bps (e.g. 12200 for AMR-NB 12.2kbps)".to_string(),
                },
                ConfigParam {
                    name: "wideband".to_string(),
                    data_type: "bool".to_string(),
                    default_value: serde_json::json!(false),
                    description: "Use AMR-WB (true) or AMR-NB (false)".to_string(),
                },
            ],
            scheduling_type: SchedulingType::DataDriven,
            timing: TimingInfo {
                wcet_us: 2000,
                bcet_us: 500,
                typical_us: 1200,
            },
            resources: ResourceInfo {
                stack_size_bytes: 16384,
                static_mem_bytes: 16384,
                requires_fpu: true,
                requires_gpu: false,
            },
            asil_level: AsilLevel::QM,
        }
    }
}

edf_core::declare_edf_module!(AMRCodec);
