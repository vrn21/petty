# Phase 4: bouvet-mcp

> MCP Server exposing Bouvet sandboxes to AI agents.

---

## Purpose

MCP (Model Context Protocol) server that:

1. Exposes sandbox management as MCP tools
2. Allows AI agents to create/execute/destroy sandboxes
3. Runs via stdio transport (standard for Claude, Cursor, etc.)
4. Integrates with `bouvet-core` for all sandbox operations

---

## MCP Protocol Overview

MCP is Anthropic's open standard for connecting AI models to external tools and data sources.

**Key Concepts:**

- **Tools**: Executable functions AI can invoke
- **Resources**: Data the AI can read (files, URLs, etc.)
- **Transports**: How client/server communicate (stdio, HTTP, WebSocket)

**SDK Choice:** `rmcp` (official Rust SDK, version 0.12+)

- Async/tokio-native
- Uses `#[tool]` and `#[tool_router]` macros
- Implements `ServerHandler` trait
- Supports stdio and HTTP transports

---

## Architecture

```
┌────────────────────────────────────────────────────────────┐
│                AI Agent (Claude, Cursor, etc.)             │
└────────────────────────────────────────────────────────────┘
                              │ stdio (JSON-RPC)
                              ▼
┌────────────────────────────────────────────────────────────┐
│                      bouvet-mcp                             │
├────────────────────────────────────────────────────────────┤
│                                                            │
│  ┌──────────────────┐     ┌────────────────────────────┐  │
│  │   BouvetServer    │────▶│      SandboxManager        │  │
│  │  (ServerHandler) │     │      (from bouvet-core)     │  │
│  └──────────────────┘     └────────────────────────────┘  │
│           │                                                │
│  ┌────────┴────────────────────────────────────────────┐  │
│  │                    MCP Tools                         │  │
│  ├──────────────────────────────────────────────────────┤  │
│  │  create_sandbox    │  destroy_sandbox                │  │
│  │  execute_code      │  run_command                    │  │
│  │  read_file         │  write_file                     │  │
│  │  list_directory    │  list_sandboxes                 │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

---

## File Structure

```
crates/bouvet-mcp/
├── Cargo.toml
└── src/
    ├── lib.rs        # Re-exports
    ├── main.rs       # Entry point (stdio server)
    ├── server.rs     # BouvetServer with tool implementations
    └── tools.rs      # Tool parameter/result types (optional)
```

---

## MCP Tools to Implement

### Sandbox Lifecycle

| Tool              | Parameters                   | Returns              | Description               |
| ----------------- | ---------------------------- | -------------------- | ------------------------- |
| `create_sandbox`  | `{memory_mib?, vcpu_count?}` | `{sandbox_id}`       | Create a new sandbox      |
| `destroy_sandbox` | `{sandbox_id}`               | `{success}`          | Destroy a sandbox         |
| `list_sandboxes`  | `{}`                         | `{sandboxes: [...]}` | List all active sandboxes |

### Code Execution

| Tool           | Parameters                     | Returns                       | Description                        |
| -------------- | ------------------------------ | ----------------------------- | ---------------------------------- |
| `execute_code` | `{sandbox_id, language, code}` | `{exit_code, stdout, stderr}` | Execute code in specified language |
| `run_command`  | `{sandbox_id, command}`        | `{exit_code, stdout, stderr}` | Run shell command                  |

### File Operations

| Tool             | Parameters                    | Returns            | Description             |
| ---------------- | ----------------------------- | ------------------ | ----------------------- |
| `read_file`      | `{sandbox_id, path}`          | `{content}`        | Read file contents      |
| `write_file`     | `{sandbox_id, path, content}` | `{success}`        | Write file              |
| `list_directory` | `{sandbox_id, path}`          | `{entries: [...]}` | List directory contents |

---

## Implementation Tasks

### Task 1: Create Crate Structure

- Set up Cargo.toml with dependencies
- Add bouvet-core as dependency
- Add rmcp with required features
- Create module files

### Task 2: Define Tool Parameter Types

- Use serde + schemars for automatic JSON schema generation
- Define request/response structs for each tool
- Example: `CreateSandboxParams`, `ExecuteCodeParams`, etc.

### Task 3: Implement BouvetServer Struct

- Wrap `SandboxManager` from bouvet-core
- Store configuration (kernel path, rootfs path, etc.)
- Implement initialization logic

### Task 4: Add MCP Tool Implementations

- Use `#[tool_router]` macro on impl block
- Use `#[tool]` macro on each method
- Return `CallToolResult` with appropriate content
- Handle errors gracefully (return error content, not panic)

### Task 5: Implement ServerHandler Trait

- Required by rmcp to handle MCP requests
- Connect tool_router to handler
- Set server capabilities (tools enabled)

### Task 6: Create Main Entry Point

- Set up tracing/logging
- Initialize SandboxManager
- Create BouvetServer
- Start stdio transport with `.serve()`

### Task 7: Add Configuration

- Command-line args or environment variables for:
  - Kernel path
  - Rootfs path
  - Firecracker binary path
  - Chroot/working directory
- Use clap or simple env vars

---

## Dependencies

```toml
[package]
name = "bouvet-mcp"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "bouvet-mcp"
path = "src/main.rs"

[dependencies]
# Internal
bouvet-core = { path = "../bouvet-core" }

# MCP SDK (latest)
rmcp = { version = "0.12", features = ["server", "transport-io"] }

# Async runtime
tokio = { version = "1", features = ["full"] }

# Serialization + schema generation
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "0.8"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# CLI (optional)
clap = { version = "4", features = ["derive"], optional = true }
```

---

## Key rmcp Patterns

### Tool Definition

```
#[tool_router]
impl BouvetServer {
    #[tool(description = "Create a new isolated sandbox")]
    async fn create_sandbox(&self, params: Parameters<CreateSandboxParams>)
        -> Result<CallToolResult, McpError>
    {
        // Implementation...
        Ok(CallToolResult::success(vec![Content::text(json_result)]))
    }
}
```

### ServerHandler Implementation

```
impl ServerHandler for BouvetServer {
    fn get_info(&self) -> ServerInfo { ... }

    async fn list_tools(&self, ...) -> Result<ListToolsResult, McpError> {
        Ok(self.tool_router.list_tools())
    }

    async fn call_tool(&self, ...) -> Result<CallToolResult, McpError> {
        self.tool_router.call_tool(request).await
    }
}
```

### Starting the Server

```
#[tokio::main]
async fn main() {
    let server = BouvetServer::new(config).await;
    server.serve(rmcp::transport::stdio()).await.unwrap();
}
```

---

## Error Handling Strategy

- Tool execution errors → Return `CallToolResult` with error content
- Sandbox not found → Return descriptive error in result
- Agent connection failed → Return error with retry suggestion
- Never panic on user input

**Error Response Format:**

```json
{
  "isError": true,
  "content": [{ "type": "text", "text": "Error: Sandbox 'xyz' not found" }]
}
```

---

## Configuration Options

| Option      | Env Var             | Default                      | Description          |
| ----------- | ------------------- | ---------------------------- | -------------------- |
| Kernel path | `BOUVET_KERNEL`      | `/var/lib/bouvet/vmlinux`     | Path to kernel image |
| Rootfs path | `BOUVET_ROOTFS`      | `/var/lib/bouvet/debian.ext4` | Path to rootfs image |
| Firecracker | `BOUVET_FIRECRACKER` | `/usr/bin/firecracker`       | Firecracker binary   |
| Chroot      | `BOUVET_CHROOT`      | `/tmp/bouvet`                 | Working directory    |

---

## Testing

### Unit Tests

- Mock SandboxManager for tool logic tests
- Test parameter validation
- Test error handling

### Integration Tests

- Full MCP round-trip with stdio
- Use rmcp client to call tools
- Verify sandbox lifecycle

### Manual Testing

```bash
# Run server
cargo run -p bouvet-mcp

# Test with MCP client (e.g., mcp-client-cli)
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | cargo run -p bouvet-mcp
```

---

## Acceptance Criteria

- [ ] Server starts and handles stdio MCP protocol
- [ ] `create_sandbox` creates a new sandbox and returns ID
- [ ] `execute_code` runs code in sandbox
- [ ] `run_command` executes shell commands
- [ ] File operations work (read/write/list)
- [ ] `destroy_sandbox` cleans up properly
- [ ] Errors are returned gracefully (no panics)
- [ ] Configuration via environment variables
- [ ] Proper logging with tracing
