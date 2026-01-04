//! Sandbox manager for lifecycle management of multiple sandboxes.

use crate::config::SandboxConfig;
use crate::error::CoreError;
use crate::sandbox::{Sandbox, SandboxId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Configuration for SandboxManager.
#[derive(Debug, Clone)]
pub struct ManagerConfig {
    /// Default kernel path for new sandboxes.
    pub kernel_path: PathBuf,
    /// Default rootfs path for new sandboxes.
    pub rootfs_path: PathBuf,
    /// Path to Firecracker binary.
    pub firecracker_path: PathBuf,
    /// Working directory for VM sockets and state.
    pub chroot_path: PathBuf,
    /// Maximum number of concurrent sandboxes (default: 100, 0 = unlimited).
    pub max_sandboxes: usize,
}

impl ManagerConfig {
    /// Create a new manager configuration.
    pub fn new(
        kernel_path: impl Into<PathBuf>,
        rootfs_path: impl Into<PathBuf>,
        firecracker_path: impl Into<PathBuf>,
        chroot_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            kernel_path: kernel_path.into(),
            rootfs_path: rootfs_path.into(),
            firecracker_path: firecracker_path.into(),
            chroot_path: chroot_path.into(),
            max_sandboxes: 100,
        }
    }
}

/// Manages multiple sandbox instances.
///
/// The SandboxManager provides a high-level API for creating, accessing,
/// and destroying sandboxes. It maintains a registry of active sandboxes
/// and ensures proper lifecycle management.
///
/// # Thread Safety
///
/// SandboxManager uses an async RwLock internally and is safe to share
/// across tasks. Multiple readers can access sandboxes concurrently,
/// while creation and destruction require exclusive access to the registry.
pub struct SandboxManager {
    sandboxes: Arc<RwLock<HashMap<SandboxId, Sandbox>>>,
    config: ManagerConfig,
}

impl SandboxManager {
    /// Create a new sandbox manager.
    pub fn new(config: ManagerConfig) -> Self {
        tracing::info!("Creating sandbox manager");
        Self {
            sandboxes: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Get the manager configuration.
    pub fn config(&self) -> &ManagerConfig {
        &self.config
    }

    /// Create a new sandbox with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Sandbox configuration
    ///
    /// # Returns
    ///
    /// The ID of the newly created sandbox.
    ///
    /// # Errors
    ///
    /// Returns an error if max_sandboxes limit is reached.
    pub async fn create(&self, config: SandboxConfig) -> Result<SandboxId, CoreError> {
        // Check sandbox limit
        if self.config.max_sandboxes > 0 {
            let current = self.sandboxes.read().await.len();
            if current >= self.config.max_sandboxes {
                return Err(CoreError::Connection(format!(
                    "max sandbox limit reached ({})",
                    self.config.max_sandboxes
                )));
            }
        }

        let sandbox = Sandbox::create(config).await?;
        let id = sandbox.id();

        let mut sandboxes = self.sandboxes.write().await;
        sandboxes.insert(id, sandbox);

        tracing::info!(sandbox_id = %id, "Sandbox registered");
        Ok(id)
    }

    /// Create a sandbox with default configuration.
    ///
    /// Uses the kernel and rootfs paths from the manager configuration.
    pub async fn create_default(&self) -> Result<SandboxId, CoreError> {
        let config = SandboxConfig::builder()
            .kernel(&self.config.kernel_path)
            .rootfs(&self.config.rootfs_path)
            .build()?;
        self.create(config).await
    }

    /// Register an externally-created sandbox.
    ///
    /// This is used to register sandboxes acquired from a warm pool.
    /// The sandbox will be tracked by the manager for lifecycle management.
    ///
    /// # Arguments
    ///
    /// * `sandbox` - A ready-to-use sandbox instance
    ///
    /// # Returns
    ///
    /// On success: The ID of the registered sandbox.
    /// On failure: A tuple of (error, sandbox) so caller can clean up.
    ///
    /// # Errors
    ///
    /// Returns an error (with the sandbox) if max_sandboxes limit is reached.
    pub async fn register(&self, sandbox: Sandbox) -> Result<SandboxId, (CoreError, Sandbox)> {
        // Check sandbox limit
        if self.config.max_sandboxes > 0 {
            let current = self.sandboxes.read().await.len();
            if current >= self.config.max_sandboxes {
                return Err((
                    CoreError::Connection(format!(
                        "max sandbox limit reached ({})",
                        self.config.max_sandboxes
                    )),
                    sandbox,
                ));
            }
        }

        let id = sandbox.id();
        let mut sandboxes = self.sandboxes.write().await;
        sandboxes.insert(id, sandbox);

        tracing::info!(sandbox_id = %id, "Sandbox registered from pool");
        Ok(id)
    }

    /// Execute a synchronous operation on a sandbox.
    ///
    /// # Arguments
    ///
    /// * `id` - Sandbox ID
    /// * `f` - Function to execute with a reference to the sandbox
    ///
    /// # Note
    ///
    /// This holds a read lock while the closure executes. For async operations,
    /// use `with_sandbox_async` instead.
    pub async fn with_sandbox<F, R>(&self, id: SandboxId, f: F) -> Result<R, CoreError>
    where
        F: FnOnce(&Sandbox) -> R,
    {
        let sandboxes = self.sandboxes.read().await;
        let sandbox = sandboxes.get(&id).ok_or(CoreError::NotFound(id))?;
        Ok(f(sandbox))
    }

    /// Execute an async operation on a sandbox.
    ///
    /// This is the primary way to interact with sandboxes for operations
    /// like executing commands or working with files.
    ///
    /// # Arguments
    ///
    /// * `id` - Sandbox ID
    /// * `f` - Async function to execute with a reference to the sandbox
    ///
    /// # Example
    ///
    /// ```ignore
    /// manager.with_sandbox_async(id, |sandbox| async move {
    ///     let result = sandbox.execute("ls -la").await?;
    ///     Ok(result)
    /// }).await?;
    /// ```
    pub async fn with_sandbox_async<F, Fut, R>(&self, id: SandboxId, f: F) -> Result<R, CoreError>
    where
        F: FnOnce(&Sandbox) -> Fut,
        Fut: std::future::Future<Output = Result<R, CoreError>>,
    {
        let sandboxes = self.sandboxes.read().await;
        let sandbox = sandboxes.get(&id).ok_or(CoreError::NotFound(id))?;
        f(sandbox).await
    }

    /// Check if a sandbox exists.
    pub async fn exists(&self, id: SandboxId) -> bool {
        let sandboxes = self.sandboxes.read().await;
        sandboxes.contains_key(&id)
    }

    /// Destroy a sandbox.
    ///
    /// This removes the sandbox from the registry and releases all resources.
    pub async fn destroy(&self, id: SandboxId) -> Result<(), CoreError> {
        let sandbox = {
            let mut sandboxes = self.sandboxes.write().await;
            sandboxes.remove(&id).ok_or(CoreError::NotFound(id))?
        };
        sandbox.destroy().await
    }

    /// Destroy all sandboxes.
    ///
    /// This is useful for cleanup during shutdown. Errors during individual
    /// sandbox destruction are logged but do not stop the process.
    pub async fn destroy_all(&self) -> Result<(), CoreError> {
        let sandboxes = {
            let mut guard = self.sandboxes.write().await;
            std::mem::take(&mut *guard)
        };

        let count = sandboxes.len();
        tracing::info!(count = count, "Destroying all sandboxes");

        for (id, sandbox) in sandboxes {
            if let Err(e) = sandbox.destroy().await {
                tracing::error!(sandbox_id = %id, error = %e, "Failed to destroy sandbox");
            }
        }

        Ok(())
    }

    /// List all sandbox IDs.
    pub async fn list(&self) -> Vec<SandboxId> {
        let sandboxes = self.sandboxes.read().await;
        sandboxes.keys().copied().collect()
    }

    /// Get the number of active sandboxes.
    pub async fn count(&self) -> usize {
        let sandboxes = self.sandboxes.read().await;
        sandboxes.len()
    }

    // =========================================================================
    // Direct Sandbox Operations
    // =========================================================================
    // These methods avoid the lifetime issues of with_sandbox_async by performing
    // the operation directly within the lock scope.

    /// Execute a shell command in a sandbox.
    ///
    /// This is a convenience method that avoids lifetime issues with closures.
    pub async fn execute(
        &self,
        id: SandboxId,
        command: &str,
    ) -> Result<crate::ExecResult, CoreError> {
        let sandboxes = self.sandboxes.read().await;
        let sandbox = sandboxes.get(&id).ok_or(CoreError::NotFound(id))?;
        sandbox.execute(command).await
    }

    /// Execute code in a specific language in a sandbox.
    ///
    /// Supported languages: python, python3, node, javascript, bash, sh
    pub async fn execute_code(
        &self,
        id: SandboxId,
        language: &str,
        code: &str,
    ) -> Result<crate::ExecResult, CoreError> {
        let sandboxes = self.sandboxes.read().await;
        let sandbox = sandboxes.get(&id).ok_or(CoreError::NotFound(id))?;
        sandbox.execute_code(language, code).await
    }

    /// Read a file from a sandbox.
    pub async fn read_file(&self, id: SandboxId, path: &str) -> Result<String, CoreError> {
        let sandboxes = self.sandboxes.read().await;
        let sandbox = sandboxes.get(&id).ok_or(CoreError::NotFound(id))?;
        sandbox.read_file(path).await
    }

    /// Write a file to a sandbox.
    pub async fn write_file(
        &self,
        id: SandboxId,
        path: &str,
        content: &str,
    ) -> Result<(), CoreError> {
        let sandboxes = self.sandboxes.read().await;
        let sandbox = sandboxes.get(&id).ok_or(CoreError::NotFound(id))?;
        sandbox.write_file(path, content).await
    }

    /// List directory contents in a sandbox.
    pub async fn list_dir(
        &self,
        id: SandboxId,
        path: &str,
    ) -> Result<Vec<crate::FileEntry>, CoreError> {
        let sandboxes = self.sandboxes.read().await;
        let sandbox = sandboxes.get(&id).ok_or(CoreError::NotFound(id))?;
        sandbox.list_dir(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ManagerConfig {
        ManagerConfig::new(
            "/path/to/vmlinux",
            "/path/to/rootfs.ext4",
            "/usr/bin/firecracker",
            "/tmp/petty",
        )
    }

    #[test]
    fn test_manager_config_new() {
        let config = test_config();
        assert_eq!(config.kernel_path, PathBuf::from("/path/to/vmlinux"));
        assert_eq!(config.rootfs_path, PathBuf::from("/path/to/rootfs.ext4"));
        assert_eq!(
            config.firecracker_path,
            PathBuf::from("/usr/bin/firecracker")
        );
        assert_eq!(config.chroot_path, PathBuf::from("/tmp/petty"));
    }

    #[tokio::test]
    async fn test_manager_empty() {
        let manager = SandboxManager::new(test_config());
        assert_eq!(manager.count().await, 0);
        assert!(manager.list().await.is_empty());
    }

    #[tokio::test]
    async fn test_manager_not_found() {
        let manager = SandboxManager::new(test_config());
        let id = SandboxId::new();
        let result = manager.destroy(id).await;
        assert!(matches!(result, Err(CoreError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_manager_exists() {
        let manager = SandboxManager::new(test_config());
        let id = SandboxId::new();
        assert!(!manager.exists(id).await);
    }
}
