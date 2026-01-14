//! BouvetServer - MCP server that exposes sandbox operations as tools.
//!
//! This module implements the core MCP server manually implementing ServerHandler
//! to expose sandbox lifecycle, code execution, and file operation tools.

use crate::config::{BouvetConfig, MAX_COMMAND_LENGTH, MAX_INPUT_SIZE_BYTES};
use crate::types::*;

use bouvet_core::{ManagerConfig, PoolConfig, SandboxConfig, SandboxManager, SandboxPool};
use rmcp::{
    handler::server::ServerHandler,
    model::*,
    service::{RequestContext, RoleServer},
    ErrorData,
};
use schemars::schema_for;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

/// MCP server for Bouvet sandbox operations.
///
/// This server exposes sandbox management, code execution, and file operations
/// as MCP tools that AI agents can invoke.
#[derive(Clone)]
pub struct BouvetServer {
    /// Sandbox manager from bouvet-core
    manager: Arc<SandboxManager>,

    /// Configuration
    config: BouvetConfig,

    /// Warm sandbox pool (optional, based on config)
    pool: Option<Arc<TokioMutex<SandboxPool>>>,
}

impl BouvetServer {
    /// Create a new BouvetServer with the given configuration.
    pub fn new(config: BouvetConfig) -> Self {
        tracing::info!(
            kernel = %config.kernel_path.display(),
            rootfs = %config.rootfs_path.display(),
            firecracker = %config.firecracker_path.display(),
            "Creating BouvetServer"
        );

        let manager_config = ManagerConfig::new(
            &config.kernel_path,
            &config.rootfs_path,
            &config.firecracker_path,
            &config.chroot_path,
        );

        let manager = Arc::new(SandboxManager::new(manager_config));

        // Create pool if enabled
        let pool = if config.pool_enabled {
            let pool_config = PoolConfig {
                min_size: config.pool_min_size,
                max_concurrent_boots: config.pool_max_boots,
                sandbox_config: SandboxConfig::builder()
                    .kernel(&config.kernel_path)
                    .rootfs(&config.rootfs_path)
                    .build()
                    .expect("valid sandbox config from validated paths"),
                ..Default::default()
            };
            tracing::info!(
                pool_enabled = true,
                min_size = config.pool_min_size,
                max_boots = config.pool_max_boots,
                "Warm pool configured"
            );
            Some(Arc::new(TokioMutex::new(SandboxPool::new(pool_config))))
        } else {
            tracing::info!("Warm pool disabled");
            None
        };

        tracing::debug!("BouvetServer created");
        Self {
            manager,
            config,
            pool,
        }
    }

    /// Start the warm pool filler task.
    ///
    /// Call this after creating the server to begin pre-warming sandboxes.
    pub async fn start_pool(&self) {
        if let Some(pool) = &self.pool {
            pool.lock().await.start();
            tracing::info!("Warm pool started");
        }
    }

    /// Gracefully shutdown the warm pool.
    ///
    /// Call this before stopping the server to clean up pooled sandboxes.
    pub async fn shutdown_pool(&self) {
        if let Some(pool) = &self.pool {
            tracing::info!("Shutting down warm pool");
            if let Err(e) = pool.lock().await.shutdown().await {
                tracing::error!(error = %e, "Pool shutdown failed");
            } else {
                tracing::debug!("Pool shutdown complete");
            }
        }
    }

    /// Get a reference to the sandbox manager.
    pub fn manager(&self) -> &SandboxManager {
        &self.manager
    }

    /// Get a cloned Arc to the sandbox manager.
    ///
    /// Use this when you need to share the manager across tasks (e.g., for cleanup).
    pub fn manager_arc(&self) -> Arc<SandboxManager> {
        Arc::clone(&self.manager)
    }

    /// Get a reference to the configuration.
    pub fn config(&self) -> &BouvetConfig {
        &self.config
    }

    /// Parse a sandbox ID from string.
    /// Uses a generic error message to prevent ID enumeration.
    fn parse_sandbox_id(id: &str) -> Result<bouvet_core::SandboxId, String> {
        uuid::Uuid::parse_str(id)
            .map(bouvet_core::SandboxId::from)
            .map_err(|_| "Sandbox not found or invalid ID".to_string())
    }

    /// Truncate sensitive content for logging.
    fn truncate_for_log(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}... ({} bytes total)", &s[..max_len], s.len())
        }
    }

    /// Validate input size.
    fn validate_size(content: &str, max_bytes: usize, field_name: &str) -> Result<(), String> {
        if content.len() > max_bytes {
            Err(format!(
                "{} exceeds maximum size ({} bytes > {} bytes)",
                field_name,
                content.len(),
                max_bytes
            ))
        } else {
            Ok(())
        }
    }

    /// Helper to create success result with JSON content
    fn json_result<T: serde::Serialize>(data: &T) -> CallToolResult {
        match serde_json::to_string_pretty(data) {
            Ok(json) => CallToolResult::success(vec![Content::text(json)]),
            Err(e) => CallToolResult::error(vec![Content::text(format!(
                "JSON serialization error: {e}"
            ))]),
        }
    }

    /// Helper to create error result
    fn error_result(message: impl Into<String>) -> CallToolResult {
        CallToolResult::error(vec![Content::text(message.into())])
    }

    /// Convert schemars RootSchema to rmcp JsonObject
    fn schema_to_json_object<T: schemars::JsonSchema>(
    ) -> Arc<serde_json::Map<String, serde_json::Value>> {
        let schema = schema_for!(T);
        let json = serde_json::to_value(&schema.schema).unwrap_or_else(|_| serde_json::json!({}));
        match json {
            serde_json::Value::Object(map) => Arc::new(map),
            _ => Arc::new(serde_json::Map::new()),
        }
    }

    /// Create an empty schema for tools with no parameters
    fn empty_schema() -> Arc<serde_json::Map<String, serde_json::Value>> {
        let mut map = serde_json::Map::new();
        map.insert("type".into(), serde_json::json!("object"));
        map.insert("properties".into(), serde_json::json!({}));
        Arc::new(map)
    }

    // ========================================================================
    // Tool Implementations
    // ========================================================================

    async fn handle_create_sandbox(
        &self,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> CallToolResult {
        let params: CreateSandboxParams = args
            .and_then(|a| serde_json::from_value(serde_json::Value::Object(a)).ok())
            .unwrap_or_default();

        let start = std::time::Instant::now();
        tracing::info!(
            memory_mib = params.memory_mib,
            vcpu_count = params.vcpu_count,
            "Tool: create_sandbox"
        );

        // Try to acquire from warm pool first
        if let Some(pool) = &self.pool {
            tracing::debug!("Attempting to acquire from warm pool");
            let acquire_result = {
                let pool_guard = pool.lock().await;
                pool_guard.acquire().await
            };

            match acquire_result {
                Ok(sandbox) => {
                    // Register the pooled sandbox with manager for lifecycle tracking
                    match self.manager.register(sandbox).await {
                        Ok(id) => {
                            tracing::info!(
                                sandbox_id = %id,
                                elapsed_ms = start.elapsed().as_millis() as u64,
                                source = "pool",
                                "Sandbox created"
                            );
                            return Self::json_result(&CreateSandboxResult {
                                sandbox_id: id.to_string(),
                            });
                        }
                        Err((e, sandbox)) => {
                            // Registration failed - must destroy sandbox to prevent leak
                            tracing::error!(error = %e, "Failed to register pooled sandbox, destroying");
                            if let Err(destroy_err) = sandbox.destroy().await {
                                tracing::error!(error = %destroy_err, "Failed to destroy unregistered sandbox");
                            }
                            // Fall through to cold-start
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!(error = %e, "Pool acquire failed, falling back to cold-start");
                }
            }
        }

        // Fallback: cold-start path (original behavior)
        tracing::debug!("Creating sandbox via cold-start");
        let mut config_builder = SandboxConfig::builder()
            .kernel(&self.config.kernel_path)
            .rootfs(&self.config.rootfs_path);

        if let Some(memory) = params.memory_mib {
            config_builder = config_builder.memory_mib(memory);
        }

        if let Some(vcpus) = params.vcpu_count {
            config_builder = config_builder.vcpu_count(vcpus);
        }

        let sandbox_config = match config_builder.build() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "Invalid sandbox configuration");
                return Self::error_result(format!("Invalid sandbox configuration: {e}"));
            }
        };

        match self.manager.create(sandbox_config).await {
            Ok(id) => {
                tracing::info!(
                    sandbox_id = %id,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    source = "cold-start",
                    "Sandbox created"
                );
                Self::json_result(&CreateSandboxResult {
                    sandbox_id: id.to_string(),
                })
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to create sandbox");
                Self::error_result(format!("Failed to create sandbox: {e}"))
            }
        }
    }

    async fn handle_destroy_sandbox(
        &self,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> CallToolResult {
        let params: DestroySandboxParams = match args
            .map(|a| serde_json::from_value(serde_json::Value::Object(a)))
            .transpose()
        {
            Ok(Some(p)) => p,
            _ => {
                tracing::warn!("destroy_sandbox called without sandbox_id");
                return Self::error_result("Missing required parameter: sandbox_id");
            }
        };

        let start = std::time::Instant::now();
        tracing::info!(sandbox_id = %params.sandbox_id, "Tool: destroy_sandbox");

        let id = match Self::parse_sandbox_id(&params.sandbox_id) {
            Ok(id) => id,
            Err(e) => {
                tracing::debug!(sandbox_id = %params.sandbox_id, "Invalid sandbox ID");
                return Self::error_result(e);
            }
        };

        match self.manager.destroy(id).await {
            Ok(()) => {
                tracing::info!(
                    sandbox_id = %id,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "Sandbox destroyed"
                );
                Self::json_result(&DestroySandboxResult { success: true })
            }
            Err(e) => {
                tracing::error!(sandbox_id = %id, error = %e, "Failed to destroy sandbox");
                Self::error_result(format!("Failed to destroy sandbox: {e}"))
            }
        }
    }

    async fn handle_list_sandboxes(&self) -> CallToolResult {
        tracing::debug!("Tool: list_sandboxes");

        let ids = self.manager.list().await;
        tracing::trace!(count = ids.len(), "Found sandboxes");

        let mut sandboxes = Vec::with_capacity(ids.len());

        for id in ids {
            if let Ok(info) = self
                .manager
                .with_sandbox(id, |sandbox| SandboxInfo {
                    sandbox_id: sandbox.id().to_string(),
                    state: sandbox.state().to_string(),
                    created_at: sandbox.created_at().to_rfc3339(),
                })
                .await
            {
                sandboxes.push(info);
            }
        }

        tracing::debug!(count = sandboxes.len(), "Listed sandboxes");
        Self::json_result(&ListSandboxesResult { sandboxes })
    }

    async fn handle_execute_code(
        &self,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> CallToolResult {
        let params: ExecuteCodeParams = match args
            .map(|a| serde_json::from_value(serde_json::Value::Object(a)))
            .transpose()
        {
            Ok(Some(p)) => p,
            _ => {
                tracing::warn!("execute_code called without required parameters");
                return Self::error_result(
                    "Missing required parameters: sandbox_id, language, code",
                );
            }
        };

        // Validate input sizes
        if let Err(e) = Self::validate_size(&params.code, MAX_INPUT_SIZE_BYTES, "code") {
            tracing::warn!(sandbox_id = %params.sandbox_id, error = %e, "Code size validation failed");
            return Self::error_result(e);
        }

        let start = std::time::Instant::now();
        tracing::info!(
            sandbox_id = %params.sandbox_id,
            language = %params.language,
            code_len = params.code.len(),
            "Tool: execute_code"
        );
        tracing::trace!(code_preview = %Self::truncate_for_log(&params.code, 200), "Code content");

        let id = match Self::parse_sandbox_id(&params.sandbox_id) {
            Ok(id) => id,
            Err(e) => {
                tracing::debug!(sandbox_id = %params.sandbox_id, "Invalid sandbox ID");
                return Self::error_result(e);
            }
        };

        // Use the new direct execute_code method
        match self
            .manager
            .execute_code(id, &params.language, &params.code)
            .await
        {
            Ok(result) => {
                tracing::info!(
                    sandbox_id = %id,
                    exit_code = result.exit_code,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "Code execution completed"
                );
                tracing::trace!(
                    stdout_len = result.stdout.len(),
                    stderr_len = result.stderr.len(),
                    "Execution output"
                );
                Self::json_result(&ExecResponse {
                    exit_code: result.exit_code,
                    stdout: result.stdout,
                    stderr: result.stderr,
                })
            }
            Err(e) => {
                tracing::error!(sandbox_id = %id, error = %e, "Code execution failed");
                Self::error_result(format!("Execution failed: {e}"))
            }
        }
    }

    async fn handle_run_command(
        &self,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> CallToolResult {
        let params: RunCommandParams = match args
            .map(|a| serde_json::from_value(serde_json::Value::Object(a)))
            .transpose()
        {
            Ok(Some(p)) => p,
            _ => {
                tracing::warn!("run_command called without required parameters");
                return Self::error_result("Missing required parameters: sandbox_id, command");
            }
        };

        // Validate command length
        if let Err(e) = Self::validate_size(&params.command, MAX_COMMAND_LENGTH, "command") {
            tracing::warn!(sandbox_id = %params.sandbox_id, error = %e, "Command size validation failed");
            return Self::error_result(e);
        }

        let start = std::time::Instant::now();
        tracing::info!(
            sandbox_id = %params.sandbox_id,
            cmd_len = params.command.len(),
            "Tool: run_command"
        );
        tracing::trace!(cmd = %Self::truncate_for_log(&params.command, 200), "Command content");

        let id = match Self::parse_sandbox_id(&params.sandbox_id) {
            Ok(id) => id,
            Err(e) => {
                tracing::debug!(sandbox_id = %params.sandbox_id, "Invalid sandbox ID");
                return Self::error_result(e);
            }
        };

        // Use the new direct execute method
        match self.manager.execute(id, &params.command).await {
            Ok(result) => {
                tracing::info!(
                    sandbox_id = %id,
                    exit_code = result.exit_code,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "Command completed"
                );
                tracing::trace!(
                    stdout_len = result.stdout.len(),
                    stderr_len = result.stderr.len(),
                    "Command output"
                );
                Self::json_result(&ExecResponse {
                    exit_code: result.exit_code,
                    stdout: result.stdout,
                    stderr: result.stderr,
                })
            }
            Err(e) => {
                tracing::error!(sandbox_id = %id, error = %e, "Command execution failed");
                Self::error_result(format!("Execution failed: {e}"))
            }
        }
    }

    async fn handle_read_file(
        &self,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> CallToolResult {
        let params: ReadFileParams = match args
            .map(|a| serde_json::from_value(serde_json::Value::Object(a)))
            .transpose()
        {
            Ok(Some(p)) => p,
            _ => {
                tracing::warn!("read_file called without required parameters");
                return Self::error_result("Missing required parameters: sandbox_id, path");
            }
        };

        tracing::info!(
            sandbox_id = %params.sandbox_id,
            path = %params.path,
            "Tool: read_file"
        );

        let id = match Self::parse_sandbox_id(&params.sandbox_id) {
            Ok(id) => id,
            Err(e) => {
                tracing::debug!(sandbox_id = %params.sandbox_id, "Invalid sandbox ID");
                return Self::error_result(e);
            }
        };

        match self.manager.read_file(id, &params.path).await {
            Ok(content) => {
                tracing::debug!(
                    sandbox_id = %id,
                    path = %params.path,
                    size = content.len(),
                    "File read successfully"
                );
                Self::json_result(&ReadFileResult { content })
            }
            Err(e) => {
                tracing::warn!(sandbox_id = %id, path = %params.path, error = %e, "Failed to read file");
                Self::error_result(format!("Failed to read file: {e}"))
            }
        }
    }

    async fn handle_write_file(
        &self,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> CallToolResult {
        let params: WriteFileParams = match args
            .map(|a| serde_json::from_value(serde_json::Value::Object(a)))
            .transpose()
        {
            Ok(Some(p)) => p,
            _ => {
                tracing::warn!("write_file called without required parameters");
                return Self::error_result(
                    "Missing required parameters: sandbox_id, path, content",
                );
            }
        };

        // Validate content size
        if let Err(e) = Self::validate_size(&params.content, MAX_INPUT_SIZE_BYTES, "content") {
            tracing::warn!(sandbox_id = %params.sandbox_id, error = %e, "Content size validation failed");
            return Self::error_result(e);
        }

        tracing::info!(
            sandbox_id = %params.sandbox_id,
            path = %params.path,
            content_len = params.content.len(),
            "Tool: write_file"
        );

        let id = match Self::parse_sandbox_id(&params.sandbox_id) {
            Ok(id) => id,
            Err(e) => {
                tracing::debug!(sandbox_id = %params.sandbox_id, "Invalid sandbox ID");
                return Self::error_result(e);
            }
        };

        match self
            .manager
            .write_file(id, &params.path, &params.content)
            .await
        {
            Ok(()) => {
                tracing::debug!(
                    sandbox_id = %id,
                    path = %params.path,
                    "File written successfully"
                );
                Self::json_result(&WriteFileResult { success: true })
            }
            Err(e) => {
                tracing::warn!(sandbox_id = %id, path = %params.path, error = %e, "Failed to write file");
                Self::error_result(format!("Failed to write file: {e}"))
            }
        }
    }

    async fn handle_list_directory(
        &self,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> CallToolResult {
        let params: ListDirectoryParams = match args
            .map(|a| serde_json::from_value(serde_json::Value::Object(a)))
            .transpose()
        {
            Ok(Some(p)) => p,
            _ => {
                tracing::warn!("list_directory called without required parameters");
                return Self::error_result("Missing required parameters: sandbox_id, path");
            }
        };

        tracing::info!(
            sandbox_id = %params.sandbox_id,
            path = %params.path,
            "Tool: list_directory"
        );

        let id = match Self::parse_sandbox_id(&params.sandbox_id) {
            Ok(id) => id,
            Err(e) => {
                tracing::debug!(sandbox_id = %params.sandbox_id, "Invalid sandbox ID");
                return Self::error_result(e);
            }
        };

        match self.manager.list_dir(id, &params.path).await {
            Ok(entries) => {
                let count = entries.len();
                let entries: Vec<FileEntryResponse> = entries
                    .into_iter()
                    .map(|e| FileEntryResponse {
                        name: e.name,
                        is_dir: e.is_dir,
                        size: e.size,
                    })
                    .collect();
                tracing::debug!(
                    sandbox_id = %id,
                    path = %params.path,
                    count,
                    "Directory listed"
                );
                Self::json_result(&ListDirectoryResult { entries })
            }
            Err(e) => {
                tracing::warn!(sandbox_id = %id, path = %params.path, error = %e, "Failed to list directory");
                Self::error_result(format!("Failed to list directory: {e}"))
            }
        }
    }

    /// Build the list of available tools
    fn build_tools_list() -> Vec<Tool> {
        vec![
            Tool::new(
                "create_sandbox",
                "Create a new isolated sandbox for code execution. Returns sandbox_id.",
                Self::schema_to_json_object::<CreateSandboxParams>(),
            ),
            Tool::new(
                "destroy_sandbox",
                "Destroy a sandbox and release all resources.",
                Self::schema_to_json_object::<DestroySandboxParams>(),
            ),
            Tool::new(
                "list_sandboxes",
                "List all active sandboxes with their metadata.",
                Self::empty_schema(),
            ),
            Tool::new(
                "execute_code",
                "Execute code in a specific language (python, node, bash, etc.) inside a sandbox.",
                Self::schema_to_json_object::<ExecuteCodeParams>(),
            ),
            Tool::new(
                "run_command",
                "Execute a shell command inside a sandbox.",
                Self::schema_to_json_object::<RunCommandParams>(),
            ),
            Tool::new(
                "read_file",
                "Read a file from the sandbox filesystem.",
                Self::schema_to_json_object::<ReadFileParams>(),
            ),
            Tool::new(
                "write_file",
                "Write a file to the sandbox filesystem.",
                Self::schema_to_json_object::<WriteFileParams>(),
            ),
            Tool::new(
                "list_directory",
                "List contents of a directory in the sandbox.",
                Self::schema_to_json_object::<ListDirectoryParams>(),
            ),
        ]
    }
}

// ============================================================================
// ServerHandler Implementation
// ============================================================================

impl ServerHandler for BouvetServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Bouvet MCP Server - Create and manage isolated code execution sandboxes. \
                 Use create_sandbox to start a new sandbox, then execute_code or run_command \
                 to run code. Use read_file, write_file, and list_directory for file operations. \
                 Don't forget to destroy_sandbox when done."
                    .into(),
            ),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: Self::build_tools_list(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let tool_name = request.name.as_ref();
        tracing::debug!(tool = tool_name, "MCP tool invocation");

        let result = match tool_name {
            "create_sandbox" => self.handle_create_sandbox(request.arguments).await,
            "destroy_sandbox" => self.handle_destroy_sandbox(request.arguments).await,
            "list_sandboxes" => self.handle_list_sandboxes().await,
            "execute_code" => self.handle_execute_code(request.arguments).await,
            "run_command" => self.handle_run_command(request.arguments).await,
            "read_file" => self.handle_read_file(request.arguments).await,
            "write_file" => self.handle_write_file(request.arguments).await,
            "list_directory" => self.handle_list_directory(request.arguments).await,
            _ => {
                tracing::warn!(tool = tool_name, "Unknown tool invoked");
                Self::error_result(format!("Unknown tool: {}", request.name))
            }
        };

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sandbox_id_valid() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let result = BouvetServer::parse_sandbox_id(uuid_str);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_sandbox_id_invalid() {
        let result = BouvetServer::parse_sandbox_id("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_build_tools_list() {
        let tools = BouvetServer::build_tools_list();
        assert_eq!(tools.len(), 8);
        assert!(tools.iter().any(|t| t.name.as_ref() == "create_sandbox"));
        assert!(tools.iter().any(|t| t.name.as_ref() == "destroy_sandbox"));
        assert!(tools.iter().any(|t| t.name.as_ref() == "execute_code"));
    }
}
