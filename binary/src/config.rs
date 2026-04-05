use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct FleetConfig {
    pub nodes: Vec<NodeConfig>,
    pub probes: Option<ProbeConfig>,
    pub load_thresholds: Option<LoadThresholds>,
    pub custom_states: Option<HashMap<String, CustomStateConfig>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomStateConfig {
    pub when: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeConfig {
    pub name: String,
    pub host: String,
    pub ssh: String,
    pub os: Option<String>,
    pub shared: Option<bool>,
    pub gpu: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProbeConfig {
    #[serde(default = "default_health_interval")]
    pub health_interval: u64,
    #[serde(default = "default_metrics_interval")]
    pub metrics_interval: u64,
    #[serde(default = "default_idle_threshold")]
    pub idle_threshold: u64,
}

fn default_health_interval() -> u64 {
    60
}
fn default_metrics_interval() -> u64 {
    120
}
fn default_idle_threshold() -> u64 {
    30
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            health_interval: 60,
            metrics_interval: 120,
            idle_threshold: 30,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoadThresholds {
    #[serde(default = "default_low")]
    pub low: u8,
    #[serde(default = "default_high")]
    pub high: u8,
}

fn default_low() -> u8 {
    30
}
fn default_high() -> u8 {
    70
}

impl Default for LoadThresholds {
    fn default() -> Self {
        Self { low: 30, high: 70 }
    }
}

impl FleetConfig {
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let config: FleetConfig =
            serde_yaml::from_str(yaml).context("Failed to parse fleet YAML config")?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let contents =
            std::fs::read_to_string(path).context("Failed to read fleet config file")?;
        Self::from_yaml(&contents)
    }

    pub fn probe_config(&self) -> ProbeConfig {
        self.probes.clone().unwrap_or_default()
    }

    pub fn load_threshold_config(&self) -> LoadThresholds {
        self.load_thresholds.clone().unwrap_or_default()
    }
}
