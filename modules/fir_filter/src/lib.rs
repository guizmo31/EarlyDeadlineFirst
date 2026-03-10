// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use edf_core::{
    AsilLevel, ConfigParam, EdfModule, ModuleMetadata, PortDescriptor, ResourceInfo,
    SchedulingType, TimingInfo,
};

pub struct FirFilterModule {
    coefficients: Vec<f32>,
    delay_line: Vec<f32>,
    initialized: bool,
}

impl Default for FirFilterModule {
    fn default() -> Self {
        Self {
            coefficients: vec![1.0],
            delay_line: Vec::new(),
            initialized: false,
        }
    }
}

impl EdfModule for FirFilterModule {
    fn init(&mut self) {
        self.delay_line = vec![0.0; self.coefficients.len()];
        self.initialized = true;
    }

    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }
        let input = inputs[0];
        let output = &mut outputs[0];
        output.clear();

        let order = self.coefficients.len();
        if self.delay_line.len() != order {
            self.delay_line.resize(order, 0.0);
        }

        for chunk in input.chunks_exact(4) {
            let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);

            for i in (1..order).rev() {
                self.delay_line[i] = self.delay_line[i - 1];
            }
            if order > 0 {
                self.delay_line[0] = sample;
            }

            let filtered: f32 = self
                .coefficients
                .iter()
                .zip(self.delay_line.iter())
                .map(|(c, d)| c * d)
                .sum();

            output.extend_from_slice(&filtered.to_le_bytes());
        }
    }

    fn configure(&mut self, params: &serde_json::Value) {
        if let Some(coeffs) = params.get("coefficients").and_then(|v| v.as_array()) {
            self.coefficients = coeffs
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();
            self.delay_line.resize(self.coefficients.len(), 0.0);
        }
    }

    fn reset(&mut self) {
        self.delay_line.fill(0.0);
        self.initialized = false;
    }

    fn metadata(&self) -> ModuleMetadata {
        ModuleMetadata {
            name: "FirFilter".to_string(),
            version: 1,
            description: "FIR (Finite Impulse Response) filter — applies convolution with configurable coefficients for signal smoothing, low-pass, high-pass, or band-pass filtering.".to_string(),
            category: "Signal Processing".to_string(),
            input_ports: vec![PortDescriptor {
                port_name: "signal_in".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Input signal samples (f32 little-endian)".to_string(),
                example_values: "1.0, 0.0, 1.0, 0.0, 1.0".to_string(),
            }],
            output_ports: vec![PortDescriptor {
                port_name: "signal_out".to_string(),
                data_type: "f32[]".to_string(),
                sample_size_bytes: 4,
                description: "Filtered output signal samples (f32 little-endian)".to_string(),
                example_values: String::new(),
            }],
            config_params: vec![ConfigParam {
                name: "coefficients".to_string(),
                data_type: "f32[]".to_string(),
                default_value: serde_json::json!([1.0]),
                description: "Filter tap weights".to_string(),
            }],
            scheduling_type: SchedulingType::Periodic,
            timing: TimingInfo {
                wcet_us: 500,
                bcet_us: 100,
                typical_us: 300,
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

edf_core::declare_edf_module!(FirFilterModule);
