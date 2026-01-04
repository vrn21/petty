//! Configuration for the MCP server.
//!
//! Configuration is loaded from environment variables with sensible defaults.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

/// Maximum size for code/content input in bytes (10 MB).
pub const MAX_INPUT_SIZE_BYTES: usize = 10 * 1024 * 1024;

/// Maximum command length in characters.
pub const MAX_COMMAND_LENGTH: usize = 1024 * 1024; // 1 MB

/// Transport mode for the MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransportMode {
    /// Stdio only (for local AI tools like Claude Desktop)
    Stdio,
    /// HTTP/SSE only (for remote AI agents)
    Http,
    /// Both stdio and HTTP (default - maximum compatibility)
    #[default]
    Both,
}

impl TransportMode {
    /// Parse from string (case-insensitive).
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "stdio" => Self::Stdio,
            "http" | "sse" | "remote" => Self::Http,
            _ => Self::Both,
        }
    }

    /// Check if stdio transport should be enabled.
    pub fn stdio_enabled(&self) -> bool {
        matches!(self, Self::Stdio | Self::Both)
    }

    /// Check if HTTP transport should be enabled.
    pub fn http_enabled(&self) -> bool {
        matches!(self, Self::Http | Self::Both)
    }
}

/// Configuration for the Petty MCP server.
#[derive(Debug, Clone)]
pub struct PettyConfig {
    /// Path to the kernel image.
    pub kernel_path: PathBuf,

    /// Path to the rootfs image.
    pub rootfs_path: PathBuf,

    /// Path to the Firecracker binary.
    pub firecracker_path: PathBuf,

    /// Working directory for VMs.
    pub chroot_path: PathBuf,

    /// Enable warm pooling for faster sandbox creation (default: true).
    pub pool_enabled: bool,

    /// Minimum warm sandboxes in pool (default: 3).
    pub pool_min_size: usize,

    /// Maximum concurrent boots during pool fill (default: 2).
    pub pool_max_boots: usize,

    /// Transport mode (default: both stdio and HTTP).
    pub transport_mode: TransportMode,

    /// HTTP server bind address.
    pub http_addr: SocketAddr,
}

/// Configuration validation error.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("kernel file not found: {0}")]
    MissingKernel(PathBuf),

    #[error("rootfs file not found: {0}")]
    MissingRootfs(PathBuf),

    #[error("firecracker binary not found: {0}")]
    MissingFirecracker(PathBuf),

    #[error("chroot parent directory not found: {0}")]
    InvalidChroot(PathBuf),
}

impl Default for PettyConfig {
    fn default() -> Self {
        Self {
            kernel_path: PathBuf::from("/var/lib/petty/vmlinux"),
            rootfs_path: PathBuf::from("/var/lib/petty/debian-devbox.ext4"),
            firecracker_path: PathBuf::from("/usr/local/bin/firecracker"),
            chroot_path: PathBuf::from("/tmp/petty"),
            pool_enabled: true,
            pool_min_size: 3,
            pool_max_boots: 2,
            transport_mode: TransportMode::Both,
            http_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080),
        }
    }
}

impl PettyConfig {
    /// Load configuration from environment variables.
    ///
    /// | Variable | Default |
    /// |----------|---------|
    /// | `PETTY_KERNEL` | `/var/lib/petty/vmlinux` |
    /// | `PETTY_ROOTFS` | `/var/lib/petty/debian-devbox.ext4` |
    /// | `PETTY_FIRECRACKER` | `/usr/local/bin/firecracker` |
    /// | `PETTY_CHROOT` | `/tmp/petty` |
    /// | `PETTY_POOL_ENABLED` | `true` |
    /// | `PETTY_POOL_MIN_SIZE` | `3` |
    /// | `PETTY_POOL_MAX_BOOTS` | `2` |
    /// | `PETTY_TRANSPORT` | `both` (stdio, http, both) |
    /// | `PETTY_HTTP_HOST` | `0.0.0.0` |
    /// | `PETTY_HTTP_PORT` | `8080` |
    pub fn from_env() -> Self {
        let default = Self::default();

        let http_host: IpAddr = std::env::var("PETTY_HTTP_HOST")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));

        let http_port: u16 = std::env::var("PETTY_HTTP_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8080);

        Self {
            kernel_path: std::env::var("PETTY_KERNEL")
                .map(PathBuf::from)
                .unwrap_or(default.kernel_path),
            rootfs_path: std::env::var("PETTY_ROOTFS")
                .map(PathBuf::from)
                .unwrap_or(default.rootfs_path),
            firecracker_path: std::env::var("PETTY_FIRECRACKER")
                .map(PathBuf::from)
                .unwrap_or(default.firecracker_path),
            chroot_path: std::env::var("PETTY_CHROOT")
                .map(PathBuf::from)
                .unwrap_or(default.chroot_path),
            pool_enabled: std::env::var("PETTY_POOL_ENABLED")
                .map(|v| v != "false" && v != "0")
                .unwrap_or(default.pool_enabled),
            pool_min_size: std::env::var("PETTY_POOL_MIN_SIZE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default.pool_min_size),
            pool_max_boots: std::env::var("PETTY_POOL_MAX_BOOTS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default.pool_max_boots),
            transport_mode: std::env::var("PETTY_TRANSPORT")
                .map(|v| TransportMode::parse(&v))
                .unwrap_or(default.transport_mode),
            http_addr: SocketAddr::new(http_host, http_port),
        }
    }

    /// Validate that all configured paths exist.
    ///
    /// Call this at startup to get clear error messages about missing files.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.kernel_path.exists() {
            return Err(ConfigError::MissingKernel(self.kernel_path.clone()));
        }

        if !self.rootfs_path.exists() {
            return Err(ConfigError::MissingRootfs(self.rootfs_path.clone()));
        }

        if !self.firecracker_path.exists() {
            return Err(ConfigError::MissingFirecracker(
                self.firecracker_path.clone(),
            ));
        }

        // chroot is typically created on demand, so just check parent exists
        if let Some(parent) = self.chroot_path.parent() {
            if !parent.exists() {
                return Err(ConfigError::InvalidChroot(self.chroot_path.clone()));
            }
        }

        Ok(())
    }

    /// Validate configuration but only log warnings instead of failing.
    ///
    /// Use this for development environments where paths may not exist yet.
    pub fn validate_warn(&self) {
        if !self.kernel_path.exists() {
            tracing::warn!("Kernel not found: {:?}", self.kernel_path);
        }

        if !self.rootfs_path.exists() {
            tracing::warn!("Rootfs not found: {:?}", self.rootfs_path);
        }

        if !self.firecracker_path.exists() {
            tracing::warn!("Firecracker not found: {:?}", self.firecracker_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PettyConfig::default();
        assert_eq!(config.kernel_path, PathBuf::from("/var/lib/petty/vmlinux"));
        assert_eq!(
            config.rootfs_path,
            PathBuf::from("/var/lib/petty/debian.ext4")
        );
        assert_eq!(
            config.firecracker_path,
            PathBuf::from("/usr/bin/firecracker")
        );
        assert_eq!(config.chroot_path, PathBuf::from("/tmp/petty"));
        assert_eq!(config.transport_mode, TransportMode::Both);
        assert_eq!(config.http_addr.port(), 8080);
    }

    #[test]
    fn test_transport_mode_parsing() {
        assert_eq!(TransportMode::parse("stdio"), TransportMode::Stdio);
        assert_eq!(TransportMode::parse("STDIO"), TransportMode::Stdio);
        assert_eq!(TransportMode::parse("http"), TransportMode::Http);
        assert_eq!(TransportMode::parse("HTTP"), TransportMode::Http);
        assert_eq!(TransportMode::parse("both"), TransportMode::Both);
        assert_eq!(TransportMode::parse("anything"), TransportMode::Both);
    }

    #[test]
    fn test_transport_mode_flags() {
        assert!(TransportMode::Stdio.stdio_enabled());
        assert!(!TransportMode::Stdio.http_enabled());

        assert!(!TransportMode::Http.stdio_enabled());
        assert!(TransportMode::Http.http_enabled());

        assert!(TransportMode::Both.stdio_enabled());
        assert!(TransportMode::Both.http_enabled());
    }

    #[test]
    fn test_from_env_uses_defaults() {
        // Clear any existing env vars
        std::env::remove_var("PETTY_KERNEL");
        std::env::remove_var("PETTY_ROOTFS");
        std::env::remove_var("PETTY_FIRECRACKER");
        std::env::remove_var("PETTY_CHROOT");
        std::env::remove_var("PETTY_TRANSPORT");
        std::env::remove_var("PETTY_HTTP_HOST");
        std::env::remove_var("PETTY_HTTP_PORT");

        let config = PettyConfig::from_env();
        let default = PettyConfig::default();

        assert_eq!(config.kernel_path, default.kernel_path);
        assert_eq!(config.rootfs_path, default.rootfs_path);
        assert_eq!(config.firecracker_path, default.firecracker_path);
        assert_eq!(config.chroot_path, default.chroot_path);
        assert_eq!(config.transport_mode, TransportMode::Both);
    }

    #[test]
    fn test_max_input_size() {
        // Ensure constants are reasonable
        assert_eq!(MAX_INPUT_SIZE_BYTES, 10 * 1024 * 1024);
        assert_eq!(MAX_COMMAND_LENGTH, 1024 * 1024);
    }
}
