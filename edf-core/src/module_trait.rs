// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

use serde::{Deserialize, Serialize};

/// Scheduling activation type for a module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SchedulingType {
    Periodic,
    DataDriven,
    Sporadic,
}

/// ASIL safety level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AsilLevel {
    QM,
    #[serde(rename = "ASIL-A")]
    AsilA,
    #[serde(rename = "ASIL-B")]
    AsilB,
    #[serde(rename = "ASIL-C")]
    AsilC,
    #[serde(rename = "ASIL-D")]
    AsilD,
}

/// Describes a single input or output port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortDescriptor {
    pub port_name: String,
    pub data_type: String,
    pub sample_size_bytes: usize,
    pub description: String,
    #[serde(default)]
    pub example_values: String,
}

/// Timing constraints for a module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingInfo {
    pub wcet_us: u64,
    pub bcet_us: u64,
    pub typical_us: u64,
}

/// Resource requirements for a module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub stack_size_bytes: u64,
    pub static_mem_bytes: u64,
    pub requires_fpu: bool,
    pub requires_gpu: bool,
}

/// Describes a configuration parameter accepted by `configure()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigParam {
    pub name: String,
    pub data_type: String,
    pub default_value: serde_json::Value,
    pub description: String,
}

/// Full metadata for a module — compatible with the builder's ModuleClass JSON
/// and enriched with MCP-like descriptive fields for AI agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleMetadata {
    pub name: String,
    pub version: u32,
    pub description: String,
    pub category: String,
    pub input_ports: Vec<PortDescriptor>,
    pub output_ports: Vec<PortDescriptor>,
    #[serde(default)]
    pub config_params: Vec<ConfigParam>,
    pub scheduling_type: SchedulingType,
    pub timing: TimingInfo,
    pub resources: ResourceInfo,
    pub asil_level: AsilLevel,
}

/// The standard trait that every EDF-schedulable module must implement.
///
/// This trait provides a uniform API so the EDF scheduler can call
/// any module (Audio, ADAS, Signal Processing, etc.) in a standard way.
///
/// # MCP Integration
/// The `metadata()` method returns a `ModuleMetadata` struct that serves
/// as the MCP-like descriptor, allowing AI agents to discover a module's
/// role, inputs/outputs, timing, and scheduling requirements.
pub trait EdfModule: Send {
    /// Initialize the module (allocate buffers, set defaults).
    fn init(&mut self);

    /// Run one processing cycle.
    ///
    /// - `inputs`: one byte slice per input port (data from upstream modules)
    /// - `outputs`: one mutable Vec per output port (module writes its results)
    fn process(&mut self, inputs: &[&[u8]], outputs: &mut [Vec<u8>]);

    /// Apply runtime configuration / calibration parameters.
    ///
    /// Accepts a JSON value so any module can define its own parameter schema.
    fn configure(&mut self, params: &serde_json::Value);

    /// Reset internal state to initial conditions.
    fn reset(&mut self);

    /// Return the module's metadata descriptor (MCP-compatible).
    fn metadata(&self) -> ModuleMetadata;
}
