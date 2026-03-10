// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use std::collections::VecDeque;

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct JitterBuffer {
    min_delay_ms: usize,
    max_delay_ms: usize,
    target_delay_ms: usize,
    buffer: VecDeque<Vec<u8>>,
    initialized: bool,
}

impl Default for JitterBuffer {
    fn default() -> Self {
        Self {
            min_delay_ms: 20,
            max_delay_ms: 100,
            target_delay_ms: 40,
            buffer: VecDeque::new(),
            initialized: false,
        }
    }
}

impl EdfModule for JitterBuffer {
    fn init(&mut self) {
        self.min_delay_ms = 20;
        self.max_delay_ms = 100;
        self.target_delay_ms = 40;
        self.buffer.clear();
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }
        let input = inputs[0];
        let output = &mut outputs[0];
        output.clear();

        // Store incoming packet if non-empty
        if !input.is_empty() {
            self.buffer.push_back(input.to_vec());
        }

        // Drop excess packets beyond max buffer capacity
        // Assume 20ms per packet: max_delay_ms / 20 = max packets
        let max_packets = self.max_delay_ms / 20;
        while self.buffer.len() > max_packets.max(1) {
            self.buffer.pop_front();
        }

        // Output oldest packet when buffer has enough data for target delay
        // target_delay_ms / 20 = target number of buffered packets
        let target_packets = self.target_delay_ms / 20;
        if self.buffer.len() >= target_packets.max(1) {
            if let Some(packet) = self.buffer.pop_front() {
                output.extend_from_slice(&packet);
            }
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(v) = params.get("min_delay_ms").and_then(|v| v.as_u64()) {
            self.min_delay_ms = v as usize;
        }
        if let Some(v) = params.get("max_delay_ms").and_then(|v| v.as_u64()) {
            self.max_delay_ms = v as usize;
        }
        if let Some(v) = params.get("target_delay_ms").and_then(|v| v.as_u64()) {
            self.target_delay_ms = v as usize;
        }
    }

    fn reset(&mut self) {
        self.min_delay_ms = 20;
        self.max_delay_ms = 100;
        self.target_delay_ms = 40;
        self.buffer.clear();
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "JitterBuffer".to_string(),
            version: 1,
            description: "De-jitter buffer for RTP voice packets. Stores incoming packets in a circular buffer and outputs the oldest packet when sufficient data is buffered.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "packet_in".to_string(),
                data_type: "u8[]".to_string(),
                sample_size_bytes: 1,
                description: "Incoming RTP voice packet data".to_string(),
                example_values: "0x80, 0x00, 0x01, ...".to_string(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "audio_out".to_string(),
                data_type: "u8[]".to_string(),
                sample_size_bytes: 1,
                description: "De-jittered audio packet data".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "min_delay_ms".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(20),
                    description: "Minimum buffer delay in milliseconds".to_string(),
                },
                ConfigParam {
                    name: "max_delay_ms".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(100),
                    description: "Maximum buffer delay in milliseconds".to_string(),
                },
                ConfigParam {
                    name: "target_delay_ms".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(40),
                    description: "Target buffer delay in milliseconds".to_string(),
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
                static_mem_bytes: 65536,
                requires_fpu: false,
                requires_gpu: false,
            },
            asil_level: AsilLevel::QM,
        }
    }
}

edf_core::declare_edf_module!(JitterBuffer);
