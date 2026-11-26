//! Resource monitoring using sysinfo.

use bg_helper_protocol::resource::{ResourceUsage, HardwareCapability, PowerState, ThermalState, WorkerState};
use sysinfo::{System, CpuRefreshKind, MemoryRefreshKind, RefreshKind};
use std::time::Duration;
use uuid::Uuid;
use chrono::Utc;

/// Resource monitor that collects system information.
pub struct ResourceMonitor {
    system: System,
    worker_id: Uuid,
}

impl ResourceMonitor {
    pub fn new(worker_id: Uuid) -> Self {
        let refresh_kind = RefreshKind::new()
            .with_cpu(CpuRefreshKind::new().with_cpu_usage())
            .with_memory(MemoryRefreshKind::new().with_ram().with_swap());
        
        let mut system = System::new();
        system.refresh_cpu_usage();
        system.refresh_memory();
        
        Self { system, worker_id }
    }

    /// Collect current resource usage snapshot.
    pub fn collect_usage(&mut self) -> ResourceUsage {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();

        let cpus = self.system.cpus();
        let cpu_utilization = if cpus.is_empty() {
            0.0
        } else {
            cpus.iter().map(|c| c.cpu_usage() as f32).sum::<f32>() / cpus.len() as f32
        };
        let cpu_cores_used = cpu_utilization / 100.0 * cpus.len() as f32;

        let total_mem = self.system.total_memory();
        let used_mem = self.system.used_memory();

        let (storage_used, storage_available) = self.collect_storage_usage();

        let power_state = self.detect_power_state();
        let thermal_state = self.detect_thermal_state();

        ResourceUsage {
            worker_id: self.worker_id,
            timestamp: Utc::now(),
            cpu_utilization_pct: cpu_utilization,
            cpu_cores_used,
            cpu_frequency_mhz: cpus.first().map(|c| c.frequency() as u32),
            gpu_utilization_pct: None,
            gpu_memory_used_bytes: None,
            gpu_temperature_c: None,
            ram_used_bytes: used_mem,
            ram_available_bytes: total_mem.saturating_sub(used_mem),
            storage_used_bytes: storage_used,
            storage_available_bytes: storage_available,
            power_state,
            thermal_state,
        }
    }

    /// Collect hardware capabilities.
    pub fn collect_capabilities(&mut self) -> Vec<HardwareCapability> {
        let mut capabilities = Vec::new();

        // CPU capability
        let cpus = self.system.cpus();
        if let Some(cpu) = cpus.first() {
            capabilities.push(HardwareCapability::Cpu {
                cores: self.system.physical_core_count().unwrap_or(cpus.len()) as u32,
                threads: cpus.len() as u32,
                frequency_mhz: cpu.frequency() as u32,
                architecture: std::env::consts::ARCH.to_string(),
                features: self.get_cpu_features(),
            });
        }

        // Memory capability
        capabilities.push(HardwareCapability::Memory {
            total_bytes: self.system.total_memory(),
            available_bytes: self.system.available_memory(),
        });

        // Storage capabilities (sysinfo 0.30 doesn't have disks() easily accessible)
        // We'll skip for now and use a placeholder
        // TODO: Use sysinfo::Disk when available

        capabilities
    }

    fn collect_storage_usage(&self) -> (u64, u64) {
        // sysinfo 0.30 doesn't expose disks easily
        // Return placeholder values
        (0, 0)
    }

    fn detect_power_state(&self) -> PowerState {
        #[cfg(target_os = "linux")]
        {
            use std::path::Path;
            if Path::new("/sys/class/power_supply").exists() {
                if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
                    for entry in entries.flatten() {
                        if let Ok(type_str) = std::fs::read_to_string(entry.path().join("type")) {
                            if type_str.trim() == "Battery" {
                                return PowerState::Battery;
                            }
                        }
                    }
                }
            }
        }
        
        #[cfg(target_os = "windows")]
        {
            // Windows: use WMI or GetSystemPowerStatus
            // Placeholder for now
        }
        
        PowerState::Ac
    }

    fn detect_thermal_state(&self) -> ThermalState {
        // sysinfo 0.30 may not have temperature()
        // Return Normal as default
        ThermalState::Normal
    }

    fn get_cpu_features(&self) -> Vec<String> {
        let mut features = Vec::new();
        #[cfg(target_arch = "x86_64")]
        {
            if is_x86_feature_detected!("avx2") { features.push("avx2".into()); }
            if is_x86_feature_detected!("avx") { features.push("avx".into()); }
            if is_x86_feature_detected!("sse4.2") { features.push("sse4.2".into()); }
            if is_x86_feature_detected!("fma") { features.push("fma".into()); }
        }
        #[cfg(target_arch = "aarch64")]
        {
            features.push("neon".into());
        }
        features
    }
}

/// Background monitor that periodically collects usage.
pub struct BackgroundMonitor {
    monitor: ResourceMonitor,
    interval: Duration,
}

impl BackgroundMonitor {
    pub fn new(worker_id: Uuid, interval: Duration) -> Self {
        Self {
            monitor: ResourceMonitor::new(worker_id),
            interval,
        }
    }

    pub async fn run<F>(mut self, mut callback: F) 
    where 
        F: FnMut(ResourceUsage) + Send + 'static,
    {
        let mut interval = tokio::time::interval(self.interval);
        loop {
            interval.tick().await;
            let usage = self.monitor.collect_usage();
            callback(usage);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_monitor_creation() {
        let mut monitor = ResourceMonitor::new(Uuid::new_v4());
        let usage = monitor.collect_usage();
        
        assert!(usage.cpu_utilization_pct >= 0.0);
        assert!(usage.cpu_utilization_pct <= 100.0);
        assert!(usage.ram_used_bytes > 0);
    }

    #[test]
    fn test_capabilities_collection() {
        let mut monitor = ResourceMonitor::new(Uuid::new_v4());
        let caps = monitor.collect_capabilities();
        
        assert!(!caps.is_empty());
        let has_cpu = caps.iter().any(|c| matches!(c, HardwareCapability::Cpu { .. }));
        let has_mem = caps.iter().any(|c| matches!(c, HardwareCapability::Memory { .. }));
        assert!(has_cpu);
        assert!(has_mem);
    }
}