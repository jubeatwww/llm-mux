use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use crate::error::AppError;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    3000
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    #[serde(default = "default_true")]
    pub supports_auto_model: bool,
    #[serde(default)]
    pub models: Vec<ModelConfig>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    #[serde(default)]
    pub rps: Option<u32>,
    #[serde(default)]
    pub rpm: Option<u32>,
    #[serde(default)]
    pub concurrent: Option<u32>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelSettings {
    pub rps: Option<u32>,
    pub rpm: Option<u32>,
    pub concurrent: Option<u32>,
    pub timeout_secs: Option<u64>,
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, AppError> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| AppError::ConfigLoad(format!("failed to read config: {e}")))?;
        toml::from_str(&content)
            .map_err(|e| AppError::ConfigLoad(format!("failed to parse config: {e}")))
    }

    pub fn model_settings(&self) -> HashMap<(String, String), ModelSettings> {
        let mut map = HashMap::new();
        for provider in &self.providers {
            for model in &provider.models {
                let key = (provider.name.clone(), model.name.clone());
                map.insert(
                    key,
                    ModelSettings {
                        rps: model.rps,
                        rpm: model.rpm,
                        concurrent: model.concurrent,
                        timeout_secs: model.timeout_secs,
                    },
                );
            }
        }
        map
    }

    pub fn provider_supports_auto_model(&self) -> HashMap<String, bool> {
        self.providers
            .iter()
            .map(|p| (p.name.clone(), p.supports_auto_model))
            .collect()
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}
