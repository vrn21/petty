//! Configuration structures for the Petty sandbox platform.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Complete configuration for the sandbox platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    /// VM manager configuration
    pub vm_manager: VMManagerConfig,
    /// Agent communication configuration
    pub agent_comms: AgentCommsConfig,
    /// Orchestrator configuration
    pub orchestrator: OrchestratorConfig,
}

/// Configuration for the VM manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMManagerConfig {
    /// Type of VM manager ("flintlock" for now)
    #[serde(default = "default_vm_manager_type")]
    pub manager_type: String,
    /// Flintlock-specific configuration
    pub flintlock: FlintlockConfig,
}

fn default_vm_manager_type() -> String {
    "flintlock".to_string()
}

/// Flintlock-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlintlockConfig {
    /// Flintlock gRPC endpoint
    #[serde(default = "default_flintlock_endpoint")]
    pub endpoint: String,
    /// Namespace for VMs
    #[serde(default = "default_namespace")]
    pub namespace: String,
    /// Path to the kernel image
    pub kernel_path: String,
    /// Container image name for the sandbox
    pub image_name: String,
    /// Connection timeout in seconds
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
}

fn default_flintlock_endpoint() -> String {
    "http://localhost:9090".to_string()
}

fn default_namespace() -> String {
    "default".to_string()
}

fn default_connect_timeout() -> u64 {
    10
}

/// Configuration for agent communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCommsConfig {
    /// vsock port the agent listens on
    #[serde(default = "default_vsock_port")]
    pub vsock_port: u32,
    /// Connection timeout in seconds
    #[serde(default = "default_agent_connect_timeout")]
    pub connect_timeout_secs: u64,
    /// Command execution timeout in seconds
    #[serde(default = "default_command_timeout")]
    pub command_timeout_secs: u64,
}

fn default_vsock_port() -> u32 {
    52000
}

fn default_agent_connect_timeout() -> u64 {
    10
}

fn default_command_timeout() -> u64 {
    300
}

/// Configuration for the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    /// Maximum number of concurrent sandboxes
    #[serde(default = "default_max_sandboxes")]
    pub max_concurrent_sandboxes: usize,
    /// Default TTL for idle sandboxes in seconds (0 = no TTL)
    #[serde(default = "default_ttl")]
    pub default_ttl_secs: u64,
    /// Cleanup interval in seconds
    #[serde(default = "default_cleanup_interval")]
    pub cleanup_interval_secs: u64,
    /// Default VM configuration
    pub defaults: VMDefaults,
}

fn default_max_sandboxes() -> usize {
    100
}

fn default_ttl() -> u64 {
    3600 // 1 hour
}

fn default_cleanup_interval() -> u64 {
    60
}

/// Default VM resource configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMDefaults {
    /// Number of vCPUs
    #[serde(default = "default_vcpu")]
    pub vcpu: u32,
    /// Memory in MB
    #[serde(default = "default_memory_mb")]
    pub memory_mb: u32,
    /// Disk size in MB
    #[serde(default = "default_disk_mb")]
    pub disk_size_mb: u32,
}

fn default_vcpu() -> u32 {
    1
}

fn default_memory_mb() -> u32 {
    512
}

fn default_disk_mb() -> u32 {
    1024
}

impl Default for VMDefaults {
    fn default() -> Self {
        Self {
            vcpu: default_vcpu(),
            memory_mb: default_memory_mb(),
            disk_size_mb: default_disk_mb(),
        }
    }
}

impl PlatformConfig {
    /// Load configuration from a TOML file.
    pub fn from_file(path: &str) -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(config::File::with_name(path))
            .build()?
            .try_deserialize()
    }

    /// Create a default configuration.
    pub fn default_config() -> Self {
        Self {
            vm_manager: VMManagerConfig {
                manager_type: default_vm_manager_type(),
                flintlock: FlintlockConfig {
                    endpoint: default_flintlock_endpoint(),
                    namespace: default_namespace(),
                    kernel_path: "/var/lib/flintlock/kernels/vmlinux-5.10".to_string(),
                    image_name: "docker.io/library/sandbox-base:v0.1".to_string(),
                    connect_timeout_secs: default_connect_timeout(),
                },
            },
            agent_comms: AgentCommsConfig {
                vsock_port: default_vsock_port(),
                connect_timeout_secs: default_agent_connect_timeout(),
                command_timeout_secs: default_command_timeout(),
            },
            orchestrator: OrchestratorConfig {
                max_concurrent_sandboxes: default_max_sandboxes(),
                default_ttl_secs: default_ttl(),
                cleanup_interval_secs: default_cleanup_interval(),
                defaults: VMDefaults::default(),
            },
        }
    }
}

impl AgentCommsConfig {
    /// Get the connection timeout as a Duration.
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout_secs)
    }

    /// Get the command timeout as a Duration.
    pub fn command_timeout(&self) -> Duration {
        Duration::from_secs(self.command_timeout_secs)
    }
}

impl OrchestratorConfig {
    /// Get the cleanup interval as a Duration.
    pub fn cleanup_interval(&self) -> Duration {
        Duration::from_secs(self.cleanup_interval_secs)
    }

    /// Get the default TTL as an Option<Duration> (None if 0).
    pub fn default_ttl(&self) -> Option<Duration> {
        if self.default_ttl_secs > 0 {
            Some(Duration::from_secs(self.default_ttl_secs))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PlatformConfig::default_config();
        assert_eq!(config.orchestrator.defaults.vcpu, 1);
        assert_eq!(config.orchestrator.defaults.memory_mb, 512);
        assert_eq!(config.agent_comms.vsock_port, 52000);
    }

    #[test]
    fn test_duration_helpers() {
        let config = AgentCommsConfig {
            vsock_port: 52000,
            connect_timeout_secs: 10,
            command_timeout_secs: 300,
        };
        assert_eq!(config.connect_timeout(), Duration::from_secs(10));
    }

    #[test]
    fn test_ttl_conversion() {
        let config = OrchestratorConfig {
            max_concurrent_sandboxes: 100,
            default_ttl_secs: 0,
            cleanup_interval_secs: 60,
            defaults: VMDefaults::default(),
        };
        assert!(config.default_ttl().is_none());

        let config_with_ttl = OrchestratorConfig {
            default_ttl_secs: 3600,
            ..config
        };
        assert_eq!(config_with_ttl.default_ttl(), Some(Duration::from_secs(3600)));
    }
}
