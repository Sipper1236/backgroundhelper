//! Worker configuration management.

use bg_helper_policy::LimitsBuilder;
use bg_helper_protocol::resource::{PowerPolicy, ResourceLimits, SharingSchedule};
use bg_helper_protocol::validation::Validatable;
use dirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

/// Worker configuration stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub worker_id: Option<Uuid>,
    pub worker_name: String,
    pub coordinator_url: Option<String>,
    pub limits: ResourceLimits,
    pub enrollment_token: Option<String>,
    pub auto_start: bool,
    pub log_level: String,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            worker_id: None,
            worker_name: format!("worker-{}", Uuid::new_v4().to_string()[..8].to_string()),
            coordinator_url: None,
            limits: bg_helper_policy::default_limits(),
            enrollment_token: None,
            auto_start: false,
            log_level: "info".into(),
        }
    }
}

/// Configuration manager with persistence.
pub struct ConfigManager {
    config: Arc<RwLock<WorkerConfig>>,
    config_path: PathBuf,
}

impl ConfigManager {
    /// Create a new config manager.
    pub async fn new() -> Result<Self, String> {
        let config_path = Self::default_config_path();
        let config = if config_path.exists() {
            Self::load_config(&config_path).await?
        } else {
            WorkerConfig::default()
        };

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
        })
    }

    /// Get default config path.
    fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("BackgroundHelper")
            .join("worker.toml")
    }

    /// Load config from file.
    async fn load_config(path: &PathBuf) -> Result<WorkerConfig, String> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read config: {}", e))?;
        toml::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))
    }

    /// Save config to file.
    pub async fn save(&self) -> Result<(), String> {
        let config = self.config.read().await;
        let content = toml::to_string_pretty(&*config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        // Ensure parent directory exists
        if let Some(parent) = self.config_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create config dir: {}", e))?;
        }

        tokio::fs::write(&self.config_path, content)
            .await
            .map_err(|e| format!("Failed to write config: {}", e))?;

        Ok(())
    }

    /// Get current config.
    pub async fn get(&self) -> WorkerConfig {
        self.config.read().await.clone()
    }

    /// Update config.
    pub async fn update<F>(&self, f: F) -> Result<(), String>
    where
        F: FnOnce(&mut WorkerConfig),
    {
        let mut config = self.config.write().await;
        f(&mut config);
        drop(config);
        self.save().await
    }

    /// Update resource limits.
    pub async fn update_limits(&self, limits: ResourceLimits) -> Result<(), String> {
        limits.validate().map_err(|e| e.to_string())?;
        self.update(|c| c.limits = limits).await
    }

    /// Update worker name.
    pub async fn update_name(&self, name: String) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err("Worker name cannot be empty".into());
        }
        if name.len() > 100 {
            return Err("Worker name too long (max 100)".into());
        }
        self.update(|c| c.worker_name = name).await
    }

    /// Set coordinator URL.
    pub async fn set_coordinator_url(&self, url: String) -> Result<(), String> {
        self.update(|c| c.coordinator_url = Some(url)).await
    }

    /// Set enrollment token.
    pub async fn set_enrollment_token(&self, token: String) -> Result<(), String> {
        self.update(|c| c.enrollment_token = Some(token)).await
    }

    /// Clear enrollment (for revocation).
    pub async fn clear_enrollment(&self) -> Result<(), String> {
        self.update(|c| {
            c.worker_id = None;
            c.coordinator_url = None;
            c.enrollment_token = None;
        })
        .await
    }

    /// Build limits interactively.
    pub fn limits_builder() -> LimitsBuilder {
        LimitsBuilder::new()
    }
}

/// Runtime configuration that combines persistent config with dynamic state.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub worker_id: Uuid,
    pub worker_name: String,
    pub limits: ResourceLimits,
    pub coordinator_url: String,
    pub enrollment_token: String,
}

impl RuntimeConfig {
    pub fn from_config(config: &WorkerConfig) -> Option<Self> {
        Some(Self {
            worker_id: config.worker_id?,
            worker_name: config.worker_name.clone(),
            limits: config.limits.clone(),
            coordinator_url: config.coordinator_url.clone()?,
            enrollment_token: config.enrollment_token.clone()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_helper_protocol::resource::ResourceLimits;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_config_manager() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("worker.toml");

        let config = WorkerConfig {
            worker_name: "test-worker".into(),
            ..Default::default()
        };

        let content = toml::to_string_pretty(&config).unwrap();
        tokio::fs::write(&config_path, content).await.unwrap();

        let manager = ConfigManager {
            config: Arc::new(RwLock::new(config)),
            config_path: config_path.clone(),
        };

        let loaded = manager.get().await;
        assert_eq!(loaded.worker_name, "test-worker");
    }

    #[test]
    fn test_limits_builder() {
        let limits = ConfigManager::limits_builder()
            .cpu_cores(2)
            .cpu_utilization(75)
            .ram_limit(2_000_000_000)
            .build()
            .unwrap();

        assert_eq!(limits.cpu_max_cores, Some(2));
        assert_eq!(limits.cpu_max_utilization_pct, Some(75));
    }
}
