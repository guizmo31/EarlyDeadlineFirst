// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct BTAudio {
    profile: String,
    sco_packet_size: usize,
    codec: String,
    initialized: bool,
}

impl Default for BTAudio {
    fn default() -> Self {
        Self {
            profile: "HFP".to_string(),
            sco_packet_size: 60,
            codec: "mSBC".to_string(),
            initialized: false,
        }
    }
}

impl EdfModule for BTAudio {
    fn init(&mut self) {
        self.profile = "HFP".to_string();
        self.sco_packet_size = 60;
        self.codec = "mSBC".to_string();
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.len() < 2 || outputs.len() < 2 {
            return;
        }
        let bt_rx = inputs[0];
        let tx_audio = inputs[1];
        let (first, rest) = outputs.split_at_mut(1);
        let rx_audio = &mut first[0];
        let bt_tx = &mut rest[0];
        rx_audio.clear();
        bt_tx.clear();

        // BT SCO header: 3 bytes (sync + type + length)
        const SCO_HEADER_SIZE: usize = 3;

        // RX path: strip BT SCO header from received data
        if bt_rx.len() > SCO_HEADER_SIZE {
            rx_audio.extend_from_slice(&bt_rx[SCO_HEADER_SIZE..]);
        }

        // TX path: add BT SCO header to audio for transmission
        if !tx_audio.is_empty() {
            let payload_len = tx_audio.len().min(self.sco_packet_size);
            // Header: sync byte, type byte, length byte
            bt_tx.push(0x01); // sync
            bt_tx.push(if self.codec == "mSBC" { 0x02 } else { 0x01 }); // type: mSBC or CVSD
            bt_tx.push(payload_len as u8); // length
            bt_tx.extend_from_slice(&tx_audio[..payload_len]);
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(v) = params.get("profile").and_then(|v| v.as_str()) {
            self.profile = v.to_string();
        }
        if let Some(v) = params.get("sco_packet_size").and_then(|v| v.as_u64()) {
            self.sco_packet_size = v as usize;
        }
        if let Some(v) = params.get("codec").and_then(|v| v.as_str()) {
            self.codec = v.to_string();
        }
    }

    fn reset(&mut self) {
        self.profile = "HFP".to_string();
        self.sco_packet_size = 60;
        self.codec = "mSBC".to_string();
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "BTAudio".to_string(),
            version: 1,
            description: "Bluetooth HFP/SCO audio interface. Adds/strips 3-byte SCO headers for BT audio format adaptation.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![
                PortDescriptor {
                    port_name: "bt_rx".to_string(),
                    data_type: "u8[]".to_string(),
                    sample_size_bytes: 1,
                    description: "Received Bluetooth audio data with SCO headers".to_string(),
                    example_values: "0x01, 0x02, 0x3C, ...".to_string(),
                },
                PortDescriptor {
                    port_name: "tx_audio".to_string(),
                    data_type: "u8[]".to_string(),
                    sample_size_bytes: 1,
                    description: "Audio data to transmit over Bluetooth".to_string(),
                    example_values: "0x7F, 0x80, ...".to_string(),
                },
            ],
            output_ports: vec![
                PortDescriptor {
                    port_name: "rx_audio".to_string(),
                    data_type: "u8[]".to_string(),
                    sample_size_bytes: 1,
                    description: "Decoded BT receive audio (SCO headers stripped)".to_string(),
                    example_values: String::new(),
                },
                PortDescriptor {
                    port_name: "bt_tx".to_string(),
                    data_type: "u8[]".to_string(),
                    sample_size_bytes: 1,
                    description: "Encoded audio for BT transmission (SCO headers added)".to_string(),
                    example_values: String::new(),
                },
            ],
            config_params: vec![
                ConfigParam {
                    name: "profile".to_string(),
                    data_type: "string".to_string(),
                    default_value: serde_json::json!("HFP"),
                    description: "Bluetooth profile: HFP or A2DP".to_string(),
                },
                ConfigParam {
                    name: "sco_packet_size".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(60),
                    description: "SCO packet payload size in bytes".to_string(),
                },
                ConfigParam {
                    name: "codec".to_string(),
                    data_type: "string".to_string(),
                    default_value: serde_json::json!("mSBC"),
                    description: "Audio codec: CVSD or mSBC".to_string(),
                },
            ],
            scheduling_type: SchedulingType::Periodic,
            timing: TimingInfo {
                wcet_us: 500,
                bcet_us: 100,
                typical_us: 250,
            },
            resources: ResourceInfo {
                stack_size_bytes: 8192,
                static_mem_bytes: 8192,
                requires_fpu: false,
                requires_gpu: false,
            },
            asil_level: AsilLevel::QM,
        }
    }
}

edf_core::declare_edf_module!(BTAudio);
