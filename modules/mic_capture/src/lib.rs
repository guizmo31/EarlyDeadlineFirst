// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct MicCaptureModule {
    sample_rate: usize,
    channels: usize,
    bit_depth: usize,
    phase: f32,
    initialized: bool,
}

impl Default for MicCaptureModule {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 1,
            bit_depth: 16,
            phase: 0.0,
            initialized: false,
        }
    }
}

impl EdfModule for MicCaptureModule {
    fn init(&mut self) {
        self.phase = 0.0;
        self.initialized = true;
    }

    fn process(&mut self, _inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if outputs.is_empty() {
            return;
        }
        let output = &mut outputs[0];
        output.clear();

        let frame_size = self.sample_rate / 1000 * 20; // 20ms frame
        let freq = 440.0_f32;
        let phase_inc = 2.0 * std::f32::consts::PI * freq / self.sample_rate as f32;

        let mut rng_state: u32 = (self.phase * 1000.0) as u32 | 1;

        for _ in 0..(frame_size * self.channels) {
            let sine = (self.phase).sin() * 0.5;

            // Simple LCG noise
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            let noise = ((rng_state >> 16) as f32 / 32768.0 - 1.0) * 0.01;

            let sample = sine + noise;
            output.extend_from_slice(&sample.to_le_bytes());
            self.phase += phase_inc;
        }

        // Keep phase in range to avoid precision loss
        if self.phase > 2.0 * std::f32::consts::PI {
            self.phase -= 2.0 * std::f32::consts::PI;
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(sr) = params.get("sample_rate").and_then(|v| v.as_u64()) {
            self.sample_rate = sr as usize;
        }
        if let Some(ch) = params.get("channels").and_then(|v| v.as_u64()) {
            self.channels = (ch as usize).max(1);
        }
        if let Some(bd) = params.get("bit_depth").and_then(|v| v.as_u64()) {
            self.bit_depth = bd as usize;
        }
    }

    fn reset(&mut self) {
        self.sample_rate = 48000;
        self.channels = 1;
        self.bit_depth = 16;
        self.phase = 0.0;
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "MicCapture".to_string(),
            version: 1,
            description: "Simulates ADC microphone capture — generates synthetic audio signal (440Hz sine with low-level noise).".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![],
            output_ports: vec![PortDescriptor {
                port_name: "audio_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Captured PCM audio samples (f32 little-endian)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "sample_rate".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(48000),
                    description: "ADC sample rate in Hz".to_string(),
                },
                ConfigParam {
                    name: "channels".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(1),
                    description: "Number of microphone channels".to_string(),
                },
                ConfigParam {
                    name: "bit_depth".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(16),
                    description: "ADC bit depth".to_string(),
                },
            ],
            scheduling_type: SchedulingType::Periodic,
            timing: TimingInfo {
                wcet_us: 200,
                bcet_us: 50,
                typical_us: 100,
            },
            resources: ResourceInfo {
                stack_size_bytes: 4096,
                static_mem_bytes: 8192,
                requires_fpu: true,
                requires_gpu: false,
            },
            asil_level: AsilLevel::QM,
        }
    }
}

edf_core::declare_edf_module!(MicCaptureModule);
