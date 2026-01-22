# Agent Internals

> **Layer**: 3 (bouvet-agent implementation)  
> **Related Code**: [`crates/bouvet-agent/src/`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/)

This document describes the internal implementation of the `bouvet-agent` — the guest daemon that runs inside Firecracker microVMs and handles JSON-RPC requests from the host.

---

## Module Structure

```
bouvet-agent/src/
├── main.rs      # Entry point, vsock listener, connection handling
├── protocol.rs  # JSON-RPC 2.0 types and data structures
├── handler.rs   # Request routing and method dispatch
├── exec.rs      # Command and code execution
└── fs.rs        # File system operations
```

| Module | Lines | Purpose |
|--------|-------|---------|
| [main.rs](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/main.rs) | 239 | Tokio runtime setup, vsock listener, connection loop |
| [protocol.rs](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/protocol.rs) | 164 | Request/Response types, error codes, parameter structs |
| [handler.rs](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/handler.rs) | 218 | Method dispatch and request handling |
| [exec.rs](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/exec.rs) | 170 | Shell command and code execution |
| [fs.rs](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/fs.rs) | 211 | File read/write/list operations |

---

## Runtime Configuration

### Tokio Runtime

The agent uses a **single-threaded** Tokio runtime for musl compatibility:

```rust
tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
```

| Aspect | Configuration | Rationale |
|--------|---------------|-----------|
| Runtime type | `current_thread` | musl compatibility — multi-thread runtime has issues with signal handling and thread spawning on musl systems |
| I/O driver | Enabled | Required for async vsock operations |
| Time driver | Enabled | Required for timeouts |

> [!NOTE]
> Using `current_thread` instead of `multi_thread` is intentional. The multi-threaded runtime can cause subtle failures on musl-based minimal guest images like Alpine or Debian slim.

### Tracing Configuration

```rust
tracing_subscriber::fmt()
    .with_writer(std::io::stderr)
    .with_env_filter(
        EnvFilter::from_default_env()
            .add_directive("bouvet_agent=debug".parse().unwrap())
    )
    .init();
```

- **Output**: stderr (visible in VM console)
- **Default level**: `debug` for `bouvet_agent` crate
- **Environment override**: `RUST_LOG` environment variable

---

## vsock Listener

### Binding

```rust
const GUEST_PORT: u32 = 52;

let addr = VsockAddr::new(VMADDR_CID_ANY, GUEST_PORT);
let listener = VsockListener::bind(addr)?;
```

| Parameter | Value | Description |
|-----------|-------|-------------|
| CID | `VMADDR_CID_ANY` (0xFFFFFFFF) | Accept from any context ID |
| Port | `52` | Fixed guest port for agent communication |

### Device Check

Before binding, the agent verifies `/dev/vsock` exists:

```rust
if !std::path::Path::new("/dev/vsock").exists() {
    return Err("/dev/vsock does not exist - vsock kernel module may not be loaded");
}
```

> [!IMPORTANT]
> The vsock kernel module (`vhost_vsock`) must be loaded in the guest kernel. This is baked into the rootfs image's kernel configuration.

---

## Connection Handling

### Accept Loop

```
┌─────────────────────────────────────────────────────────────┐
│                     Accept Loop                              │
│                                                              │
│   loop {                                                     │
│       listener.accept().await ──┬──▶ handle_connection()    │
│                                 │                            │
│                                 └──▶ log error, continue     │
│   }                                                          │
└─────────────────────────────────────────────────────────────┘
```

Connections are handled **inline** (not spawned as separate tasks) due to the current-thread runtime. This is acceptable because:
1. Only one host connection is typically active per VM
2. Requests are processed sequentially within a sandbox

### CONNECT Handshake

Firecracker's vsock implementation uses a CONNECT handshake protocol. The agent handles this transparently:

```
┌──────────────┐                      ┌──────────────┐
│     Host     │                      │    Agent     │
└──────┬───────┘                      └──────┬───────┘
       │                                      │
       │  "CONNECT 52\n"                      │
       │ ────────────────────────────────────▶│
       │                                      │
       │  "OK 52\n"                           │
       │ ◀────────────────────────────────────│
       │                                      │
       │  JSON-RPC requests...                │
       │ ◀───────────────────────────────────▶│
```

The handshake detection logic:

```rust
let trimmed = line.trim();
if let Some(port_str) = trimmed.strip_prefix("CONNECT ") {
    // Parse port, send "OK {port}\n"
    writer.write_all(format!("OK {}\n", port).as_bytes()).await?;
} else if !trimmed.is_empty() {
    // First line is already a JSON request, process it
}
```

---

## Request Processing Pipeline

### Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Request Processing                            │
│                                                                      │
│   read_line()                                                        │
│       │                                                              │
│       ▼                                                              │
│   serde_json::from_str::<Request>()                                  │
│       │                                                              │
│       ├──▶ Parse Error ──▶ Response::error(PARSE_ERROR)             │
│       │                                                              │
│       ▼                                                              │
│   handle_request(req)                                                │
│       │                                                              │
│       ▼                                                              │
│   match req.method {                                                 │
│       "ping"       ──▶ Response::success({pong: true})              │
│       "exec"       ──▶ handle_exec()                                │
│       "exec_code"  ──▶ handle_exec_code()                           │
│       "read_file"  ──▶ handle_read_file()                           │
│       "write_file" ──▶ handle_write_file()                          │
│       "list_dir"   ──▶ handle_list_dir()                            │
│       _            ──▶ Response::error(METHOD_NOT_FOUND)            │
│   }                                                                  │
│       │                                                              │
│       ▼                                                              │
│   serde_json::to_string(&response)                                   │
│       │                                                              │
│       ▼                                                              │
│   write_line() + flush()                                             │
└─────────────────────────────────────────────────────────────────────┘
```

### Method Dispatch

The [handle_request](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/handler.rs#L23-61) function routes requests to handlers:

| Method | Handler | Module |
|--------|---------|--------|
| `ping` | inline | handler.rs |
| `exec` | `handle_exec()` | handler.rs → exec.rs |
| `exec_code` | `handle_exec_code()` | handler.rs → exec.rs |
| `read_file` | `handle_read_file()` | handler.rs → fs.rs |
| `write_file` | `handle_write_file()` | handler.rs → fs.rs |
| `list_dir` | `handle_list_dir()` | handler.rs → fs.rs |

---

## Code Execution (exec.rs)

### Shell Command Execution

The [exec_command](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/exec.rs#L36-69) function executes shell commands:

```rust
Command::new("sh")
    .args(["-c", cmd])
    .output()
```

| Feature | Implementation |
|---------|----------------|
| Shell | `/bin/sh` |
| Output capture | stdout + stderr |
| Exit code | From process status, -1 on spawn failure |
| Output limit | 1 MB (truncated with UTF-8 boundary preservation) |

### Code Execution

The [exec_code](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/exec.rs#L84-135) function executes code in various languages:

| Language Alias | Interpreter | Command |
|----------------|------------|---------|
| `python`, `python3` | Python 3 | `python3 -c <code>` |
| `node`, `javascript`, `js` | Node.js | `node -e <code>` |
| `bash` | Bash | `bash -c <code>` |
| `sh` | POSIX shell | `sh -c <code>` |

> [!NOTE]
> Unlike some sandboxing systems, code is passed directly via `-c`/`-e` flags rather than written to temporary files. This avoids filesystem overhead for simple code snippets.

### Output Truncation

To prevent memory exhaustion, output is truncated to 1 MB:

```rust
const MAX_OUTPUT_SIZE: usize = 1024 * 1024;  // 1 MB

fn truncate_output(s: String, max_bytes: usize) -> String {
    if s.len() <= max_bytes { return s; }
    
    // Find valid UTF-8 boundary
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    
    let mut truncated = s[..end].to_string();
    truncated.push_str("\n... [output truncated]");
    truncated
}
```

---

## File Operations (fs.rs)

### Read File

The [read_file](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/fs.rs#L22-49) function:

```rust
const MAX_READ_SIZE: u64 = 10 * 1024 * 1024;  // 10 MB

pub fn read_file(path: &str) -> Result<String, String> {
    // 1. Check file size
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_READ_SIZE {
        return Err("file too large");
    }
    
    // 2. Read content
    fs::read_to_string(path)
}
```

| Constraint | Value | Reason |
|------------|-------|--------|
| Max size | 10 MB | Prevent memory exhaustion |
| Encoding | UTF-8 | JSON-RPC transport requires text |

### Write File

The [write_file](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/fs.rs#L61-85) function:

```rust
pub fn write_file(path: &str, content: &str) -> Result<bool, String> {
    // 1. Create parent directories if needed
    if let Some(parent) = Path::new(path).parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    
    // 2. Write content
    fs::write(path, content)?;
    Ok(true)
}
```

| Feature | Behavior |
|---------|----------|
| Parent directories | Auto-created |
| Overwrite | Yes (replaces existing) |
| Permissions | Default (umask) |

### List Directory

The [list_dir](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/fs.rs#L94-133) function:

```rust
pub fn list_dir(path: &str) -> Result<Vec<FileEntry>, String> {
    let entries = fs::read_dir(path)?;
    
    let mut result = Vec::new();
    for entry in entries {
        result.push(FileEntry {
            name: entry.file_name(),
            is_dir: metadata.is_dir(),
            size: if metadata.is_file() { metadata.len() } else { 0 },
        });
    }
    
    // Sort by name for consistent output
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}
```

| Feature | Behavior |
|---------|----------|
| Sorting | Alphabetical by name |
| Size | Bytes for files, 0 for directories |
| Hidden files | Included (no filtering) |

---

## Protocol Types (protocol.rs)

### Request Structure

```rust
pub struct Request {
    pub jsonrpc: String,      // Must be "2.0"
    pub id: u64,              // Request identifier
    pub method: String,       // Method name
    pub params: Value,        // Optional parameters (default: null)
}
```

### Response Structure

```rust
pub struct Response {
    pub jsonrpc: String,              // Always "2.0"
    pub id: u64,                      // Matches request
    pub result: Option<Value>,        // Present on success
    pub error: Option<RpcError>,      // Present on failure
}

pub struct RpcError {
    pub code: i32,            // Error code
    pub message: String,      // Human-readable message
    pub data: Option<Value>,  // Additional data (optional)
}
```

### ExecResult Type

```rust
pub struct ExecResult {
    pub exit_code: i32,    // -1 if spawn failed
    pub stdout: String,    // Standard output
    pub stderr: String,    // Standard error
}
```

### FileEntry Type

```rust
pub struct FileEntry {
    pub name: String,      // File or directory name
    pub is_dir: bool,      // True if directory
    pub size: u64,         // Size in bytes (0 for directories)
}
```

---

## Error Handling

### Error Codes

| Code | Constant | Description |
|------|----------|-------------|
| -32700 | `PARSE_ERROR` | Invalid JSON received |
| -32600 | `INVALID_REQUEST` | Not a valid request object |
| -32601 | `METHOD_NOT_FOUND` | Unknown method |
| -32602 | `INVALID_PARAMS` | Wrong or missing parameters |
| -32603 | `INTERNAL_ERROR` | Server-side error |

### Error Response Creation

```rust
Response::error(id, error_codes::INTERNAL_ERROR, "error message")
```

---

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `tokio` | workspace | Async runtime |
| `tokio-vsock` | 0.7 | VirtIO socket support |
| `serde` | workspace | Serialization traits |
| `serde_json` | workspace | JSON parsing/generation |
| `tracing` | workspace | Structured logging |
| `tracing-subscriber` | workspace | Log output formatting |

---

## Debugging Tips

### Enable Verbose Logging

Set the environment variable before VM boot:
```bash
RUST_LOG=bouvet_agent=trace
```

### Check Agent Status

From inside the VM:
```bash
systemctl status bouvet-agent
journalctl -u bouvet-agent -f
```

### Test vsock Manually

From guest (if needed):
```bash
# Check device exists
ls -la /dev/vsock

# Check agent is listening
ss -l | grep vsock
```

---

## See Also

- [AGENT_PROTOCOL.md](file:///Users/vrn21/Developer/rust/petty/docs/internals/AGENT_PROTOCOL.md) — JSON-RPC protocol specification
- [VSOCK_COMMUNICATION.md](file:///Users/vrn21/Developer/rust/petty/docs/internals/VSOCK_COMMUNICATION.md) — vsock transport details
- [ROOTFS_IMAGE.md](file:///Users/vrn21/Developer/rust/petty/docs/internals/ROOTFS_IMAGE.md) — Guest image and systemd service
