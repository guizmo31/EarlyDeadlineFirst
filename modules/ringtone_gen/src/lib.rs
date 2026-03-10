// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct RingtoneGen {
    frequency_hz: f32,
    pattern: String,
    on_ms: usize,
    off_ms: usize,
    sample_rate: usize,
    phase: f32,
    sample_counter: usize,
    initialized: bool,
}

impl Default for RingtoneGen {
    fn default() -> Self {
        Self {
            frequency_hz: 440.0,
            pattern: "alternating".to_string(),
            on_ms: 1000,
            off_ms: 1000,
            sample_rate: 48000,
            phase: 0.0,
            sample_counter: 0,
            initialized: false,
        }
    }
}

impl EdfModule for RingtoneGen {
    fn init(&mut self) {
        self.frequency_hz = 440.0;
        self.pattern = "alternating".to_string();
        self.on_ms = 1000;
        self.off_ms = 1000;
        self.sample_rate = 48000;
        self.phase = 0.0;
        self.sample_counter = 0;
        self.initialized = true;
    }

    fn process(&mut self, _inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if outputs.is_empty() {
            return;
        }
        let output = &mut outputs[0];
        output.clear();

        if self.sample_rate == 0 {
            return;
        }

        // Generate one frame of audio (20ms worth of samples)
        let frame_samples = self.sample_rate / 50; // 20ms frame
        let on_samples = self.on_ms * self.sample_rate / 1000;
        let off_samples = self.off_ms * self.sample_rate / 1000;
        let cycle_samples = on_samples + off_samples;
        let phase_increment = 2.0 * std::f32::consts::PI * self.frequency_hz / self.sample_rate as f32;

        for _ in 0..frame_samples {
            let is_on = match self.pattern.as_str() {
                "continuous" => true,
                "alternating" => {
                    if cycle_samples == 0 {
                        true
                    } else {
                        (self.sample_counter % cycle_samples) < on_samples
                    }
                }
                "melody" => {
                    // Simple melody: alternate between two frequencies via on/off
                    if cycle_samples == 0 {
                        true
                    } else {
                        (self.sample_counter % cycle_samples) < on_samples
                    }
                }
                _ => true,
            };

            let sample = if is_on {
                let s = self.phase.sin();
                self.phase += phase_increment;
                if self.phase > 2.0 * std::f32::consts::PI {
                    self.phase -= 2.0 * std::f32::consts::PI;
                }
                s
            } else {
                0.0
            };

            self.sample_counter += 1;
            output.extend_from_slice(&sample.to_le_bytes());
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(v) = params.get("frequency_hz").and_then(|v| v.as_f64()) {
            self.frequency_hz = v as f32;
        }
        if let Some(v) = params.get("pattern").and_then(|v| v.as_str()) {
            self.pattern = v.to_string();
        }
        if let Some(v) = params.get("on_ms").and_then(|v| v.as_u64()) {
            self.on_ms = v as usize;
        }
        if let Some(v) = params.get("off_ms").and_then(|v| v.as_u64()) {
            self.off_ms = v as usize;
        }
        if let Some(v) = params.get("sample_rate").and_then(|v| v.as_u64()) {
            self.sample_rate = v as usize;
        }
    }

    fn reset(&mut self) {
        self.frequency_hz = 440.0;
        self.pattern = "alternating".to_string();
        self.on_ms = 1000;
        self.off_ms = 1000;
        self.sample_rate = 48000;
        self.phase = 0.0;
        self.sample_counter = 0;
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "RingtoneGen".to_string(),
            version: 1,
            description: "Generates ringtone audio patterns. Produces sine wave tones with configurable frequency and on/off patterns.".to_string(),
            category: "Audio".to_string(),
            input_ports: vec![],
            output_ports: vec![PortDescriptor {
                port_name: "audio_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Generated ringtone PCM audio samples (f32 little-endian)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![
                ConfigParam {
                    name: "frequency_hz".to_string(),
                    data_type: "f32".to_string(),
                    default_value: serde_json::json!(440.0),
                    description: "Tone frequency in Hz".to_string(),
                },
                ConfigParam {
                    name: "pattern".to_string(),
                    data_type: "string".to_string(),
                    default_value: serde_json::json!("alternating"),
                    description: "Ringtone pattern: continuous, alternating, or melody".to_string(),
                },
                ConfigParam {
                    name: "on_ms".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(1000),
                    description: "Tone on duration in milliseconds".to_string(),
                },
                ConfigParam {
                    name: "off_ms".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(1000),
                    description: "Tone off duration in milliseconds".to_string(),
                },
                ConfigParam {
                    name: "sample_rate".to_string(),
                    data_type: "usize".to_string(),
                    default_value: serde_json::json!(48000),
                    description: "Output sample rate in Hz".to_string(),
                },
            ],
            scheduling_type: SchedulingType::Periodic,
            timing: TimingInfo {
                wcet_us: 300,
                bcet_us: 80,
                typical_us: 150,
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

edf_core::declare_edf_module!(RingtoneGen);
