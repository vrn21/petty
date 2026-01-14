//! Sandbox type - a running microVM with agent connection.

use crate::client::{AgentClient, ExecResult, FileEntry};
use crate::config::SandboxConfig;
use crate::error::CoreError;
use chrono::{DateTime, Utc};
use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Unique identifier for a sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SandboxId(Uuid);

impl SandboxId {
    /// Create a new random sandbox ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Get the underlying UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for SandboxId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SandboxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for SandboxId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

/// Current state of a sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxState {
    /// Sandbox is being created (VM booting, agent connecting).
    Creating,
    /// Sandbox is ready for commands.
    Ready,
    /// Sandbox is destroyed.
    Destroyed,
}

impl fmt::Display for SandboxState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Creating => write!(f, "Creating"),
            Self::Ready => write!(f, "Ready"),
            Self::Destroyed => write!(f, "Destroyed"),
        }
    }
}

/// A running sandbox with VM and agent connection.
///
/// A sandbox represents a complete isolated execution environment consisting of:
/// - A Firecracker microVM
/// - A connected guest agent
///
/// Use the methods on this type to execute commands and work with files
/// in the isolated environment.
pub struct Sandbox {
    id: SandboxId,
    vm: bouvet_vm::VirtualMachine,
    client: Arc<Mutex<AgentClient>>,
    config: SandboxConfig,
    state: SandboxState,
    created_at: DateTime<Utc>,
}

impl Sandbox {
    /// Create a new sandbox (called by SandboxManager).
    ///
    /// This will:
    /// 1. Create and boot a microVM
    /// 2. Wait for the guest agent to start
    /// 3. Connect to the agent via vsock
    /// 4. Verify the agent is responsive
    pub(crate) async fn create(config: SandboxConfig) -> Result<Self, CoreError> {
        let id = SandboxId::new();
        let start = std::time::Instant::now();
        tracing::info!(
            sandbox_id = %id,
            vcpus = config.vcpu_count,
            memory_mib = config.memory_mib,
            vsock_cid = config.vsock_cid,
            "Creating sandbox"
        );

        // Generate unique vsock config with per-VM UDS path
        let vsock_config =
            bouvet_vm::VsockConfig::for_vm(config.vsock_cid, &config.chroot_path, &id.to_string());
        tracing::debug!(
            sandbox_id = %id,
            uds_path = %vsock_config.uds_path.display(),
            "Generated vsock config"
        );

        // Ensure vsock directory exists
        if let Some(parent) = vsock_config.uds_path.parent() {
            tracing::trace!(sandbox_id = %id, path = %parent.display(), "Creating vsock directory");
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                tracing::error!(sandbox_id = %id, error = %e, "Failed to create vsock directory");
                CoreError::Connection(format!("Failed to create vsock directory: {}", e))
            })?;
        }

        // 1. Build VM config with unique vsock path
        tracing::debug!(sandbox_id = %id, "Building VM configuration");
        let vm_config = bouvet_vm::VmBuilder::new()
            .vcpus(config.vcpu_count)
            .memory_mib(config.memory_mib)
            .kernel(&config.kernel_path)
            .rootfs(&config.rootfs_path)
            .chroot_path(&config.chroot_path)
            .with_vsock_config(vsock_config)
            .build_config();

        // 2. Create and boot VM with the same ID as the sandbox
        tracing::debug!(sandbox_id = %id, "Creating and booting VM");
        let vm = match bouvet_vm::VirtualMachine::create_with_id(id.as_uuid(), vm_config).await {
            Ok(vm) => vm,
            Err(e) => {
                tracing::error!(sandbox_id = %id, error = %e, "VM creation failed");
                // Cleanup directory if VM creation fails
                let vsock_dir = config.chroot_path.join(id.to_string());
                let _ = tokio::fs::remove_dir_all(&vsock_dir).await;
                return Err(e.into());
            }
        };
        tracing::debug!(
            sandbox_id = %id,
            elapsed_ms = start.elapsed().as_millis() as u64,
            "VM created and started"
        );

        // 3. Get vsock path and connect to agent
        let vsock_path = vm
            .vsock_uds_path()
            .ok_or_else(|| CoreError::Connection("vsock not configured".into()))?;

        tracing::debug!(sandbox_id = %id, path = %vsock_path.display(), "Connecting to agent");
        let mut client = AgentClient::connect(vsock_path).await?;
        tracing::debug!(sandbox_id = %id, "Agent connected");

        // 4. Verify agent is responsive
        tracing::trace!(sandbox_id = %id, "Pinging agent");
        client.ping().await?;
        tracing::info!(
            sandbox_id = %id,
            elapsed_ms = start.elapsed().as_millis() as u64,
            "Sandbox ready"
        );

        Ok(Self {
            id,
            vm,
            client: Arc::new(Mutex::new(client)),
            config,
            state: SandboxState::Ready,
            created_at: Utc::now(),
        })
    }

    /// Get the sandbox ID.
    pub fn id(&self) -> SandboxId {
        self.id
    }

    /// Get the current state.
    pub fn state(&self) -> SandboxState {
        self.state
    }

    /// Get the creation timestamp.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Get the configuration used to create this sandbox.
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    /// Execute a shell command.
    ///
    /// # Arguments
    ///
    /// * `cmd` - Shell command to execute
    ///
    /// # Returns
    ///
    /// The execution result including exit code, stdout, and stderr.
    pub async fn execute(&self, cmd: &str) -> Result<ExecResult, CoreError> {
        tracing::debug!(sandbox_id = %self.id, cmd = %cmd, "Executing command");
        self.ensure_ready()?;
        let mut client = self.client.lock().await;
        let result = client.exec(cmd).await;
        if let Ok(ref r) = result {
            tracing::debug!(
                sandbox_id = %self.id,
                exit_code = r.exit_code,
                stdout_len = r.stdout.len(),
                stderr_len = r.stderr.len(),
                "Command completed"
            );
        }
        result
    }

    /// Execute code in a specific language.
    ///
    /// # Arguments
    ///
    /// * `lang` - Language identifier (python, python3, node, javascript, bash, sh)
    /// * `code` - Code to execute
    ///
    /// # Returns
    ///
    /// The execution result including exit code, stdout, and stderr.
    pub async fn execute_code(&self, lang: &str, code: &str) -> Result<ExecResult, CoreError> {
        tracing::debug!(sandbox_id = %self.id, lang = %lang, code_len = code.len(), "Executing code");
        self.ensure_ready()?;
        let mut client = self.client.lock().await;
        let result = client.exec_code(lang, code).await;
        if let Ok(ref r) = result {
            tracing::debug!(
                sandbox_id = %self.id,
                exit_code = r.exit_code,
                stdout_len = r.stdout.len(),
                stderr_len = r.stderr.len(),
                "Code execution completed"
            );
        }
        result
    }

    /// Read a file from the guest filesystem.
    ///
    /// # Arguments
    ///
    /// * `path` - Absolute path to the file
    ///
    /// # Returns
    ///
    /// The file contents as a string.
    pub async fn read_file(&self, path: &str) -> Result<String, CoreError> {
        tracing::debug!(sandbox_id = %self.id, path = %path, "Reading file");
        self.ensure_ready()?;
        let mut client = self.client.lock().await;
        let result = client.read_file(path).await;
        if let Ok(ref content) = result {
            tracing::trace!(sandbox_id = %self.id, size = content.len(), "File read");
        }
        result
    }

    /// Write a file to the guest filesystem.
    ///
    /// # Arguments
    ///
    /// * `path` - Absolute path to the file
    /// * `content` - Content to write
    pub async fn write_file(&self, path: &str, content: &str) -> Result<(), CoreError> {
        tracing::debug!(sandbox_id = %self.id, path = %path, content_len = content.len(), "Writing file");
        self.ensure_ready()?;
        let mut client = self.client.lock().await;
        client.write_file(path, content).await
    }

    /// List directory contents.
    ///
    /// # Arguments
    ///
    /// * `path` - Absolute path to the directory
    ///
    /// # Returns
    ///
    /// A list of file entries in the directory.
    pub async fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>, CoreError> {
        tracing::debug!(sandbox_id = %self.id, path = %path, "Listing directory");
        self.ensure_ready()?;
        let mut client = self.client.lock().await;
        let result = client.list_dir(path).await;
        if let Ok(ref entries) = result {
            tracing::trace!(sandbox_id = %self.id, count = entries.len(), "Directory listed");
        }
        result
    }

    /// Check if the sandbox is healthy and responsive.
    ///
    /// This pings the agent to verify it's still running and responsive.
    /// Returns true if the agent responds, false otherwise.
    pub async fn is_healthy(&self) -> bool {
        if self.state != SandboxState::Ready {
            tracing::trace!(sandbox_id = %self.id, state = ?self.state, "Health check: not ready");
            return false;
        }
        let mut client = match self.client.try_lock() {
            Ok(c) => c,
            Err(_) => {
                tracing::trace!(sandbox_id = %self.id, "Health check: client busy, assuming healthy");
                return true; // Client busy = still working
            }
        };
        let healthy = client.ping().await.is_ok();
        tracing::trace!(sandbox_id = %self.id, healthy, "Health check completed");
        healthy
    }

    /// Destroy the sandbox.
    ///
    /// This stops the VM and releases all resources.
    pub async fn destroy(mut self) -> Result<(), CoreError> {
        let start = std::time::Instant::now();
        tracing::info!(sandbox_id = %self.id, "Destroying sandbox");
        self.state = SandboxState::Destroyed;

        tracing::debug!(sandbox_id = %self.id, "Stopping VM");
        self.vm.destroy().await?;

        // Clean up vsock directory
        let vsock_dir = self.config.chroot_path.join(self.id.to_string());
        tracing::debug!(sandbox_id = %self.id, path = %vsock_dir.display(), "Removing sandbox directory");
        if let Err(e) = tokio::fs::remove_dir_all(&vsock_dir).await {
            tracing::warn!(sandbox_id = %self.id, error = %e, "Failed to remove sandbox directory");
        }

        tracing::info!(
            sandbox_id = %self.id,
            elapsed_ms = start.elapsed().as_millis() as u64,
            "Sandbox destroyed"
        );
        Ok(())
    }

    /// Ensure the sandbox is in the Ready state.
    fn ensure_ready(&self) -> Result<(), CoreError> {
        if self.state != SandboxState::Ready {
            return Err(CoreError::InvalidState {
                expected: "Ready".into(),
                actual: format!("{:?}", self.state),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_id_display() {
        let id = SandboxId::new();
        let s = format!("{}", id);
        // UUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
        assert_eq!(s.len(), 36);
        assert!(s.contains('-'));
    }

    #[test]
    fn test_sandbox_state_display() {
        assert_eq!(format!("{}", SandboxState::Creating), "Creating");
        assert_eq!(format!("{}", SandboxState::Ready), "Ready");
        assert_eq!(format!("{}", SandboxState::Destroyed), "Destroyed");
    }

    #[test]
    fn test_sandbox_id_from_uuid() {
        let uuid = Uuid::new_v4();
        let id: SandboxId = uuid.into();
        assert_eq!(format!("{}", id), format!("{}", uuid));
    }
}
