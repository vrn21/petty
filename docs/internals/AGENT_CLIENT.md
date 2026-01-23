# AgentClient Communication

> **Layer**: 4 (bouvet-core)  
> **Source**: [`crates/bouvet-core/src/client.rs`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-core/src/client.rs)

This document describes the host-side client (`AgentClient`) for communicating with `bouvet-agent` running inside a Firecracker VM via the vsock interface.

---

## Overview

The `AgentClient` is the host-side component that connects to the guest agent via Firecracker's vsock Unix Domain Socket and exchanges JSON-RPC 2.0 messages.

```
┌────────────────────────┐              ┌────────────────────────┐
│     Host (bouvet-core)  │              │   Guest (bouvet-agent) │
│                        │              │                        │
│  ┌─────────────────┐   │              │   ┌─────────────────┐   │
│  │   AgentClient   │   │    vsock     │   │  JSON-RPC Server│   │
│  │                 │◀──┼──────────────┼──▶│  (port 52)      │   │
│  │  (BufReader +   │   │              │   │                 │   │
│  │   BufWriter)    │   │              │   └─────────────────┘   │
│  └─────────────────┘   │              │                        │
└────────────────────────┘              └────────────────────────┘
```

---

## Internal Structure

```rust
pub struct AgentClient {
    reader: BufReader<tokio::io::ReadHalf<UnixStream>>,
    writer: BufWriter<tokio::io::WriteHalf<UnixStream>>,
    next_id: u64,
}
```

| Field | Type | Purpose |
|-------|------|---------|
| `reader` | `BufReader<ReadHalf<UnixStream>>` | Buffered reader for incoming responses |
| `writer` | `BufWriter<WriteHalf<UnixStream>>` | Buffered writer for outgoing requests |
| `next_id` | `u64` | Auto-incrementing JSON-RPC request ID |

---

## Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `GUEST_PORT` | `52` | The vsock port that bouvet-agent listens on |
| `CONNECT_TIMEOUT` | `10s` | Total timeout for connection (including retries) |
| `RETRY_INTERVAL` | `100ms` | Interval between connection retry attempts |
| `RPC_TIMEOUT` | `30s` | Timeout for individual RPC calls |

---

## Connection Establishment

### Public API

```rust
pub async fn connect(vsock_path: &Path) -> Result<Self, CoreError>
```

### Connection Flow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          connect(vsock_path)                            │
└───────────────────────────────────┬─────────────────────────────────────┘
                                    │
                          ┌─────────▼─────────┐
                          │  Record start time │
                          └─────────┬─────────┘
                                    │
              ┌─────────────────────▼─────────────────────┐
              │                 RETRY LOOP                 │
              │  (100ms interval, 10s total timeout)       │
              └─────────────────────┬─────────────────────┘
                                    │
                          ┌─────────▼─────────┐
                          │  try_connect()     │
                          └─────────┬─────────┘
                                    │
                   ┌────────────────┴────────────────┐
                   │                                  │
              ┌────▼────┐                       ┌─────▼─────┐
              │ Success │                       │  Failure  │
              └────┬────┘                       └─────┬─────┘
                   │                                  │
          ┌────────▼────────┐              ┌──────────▼──────────┐
          │ Return client   │              │ Elapsed >= 10s?     │
          └─────────────────┘              └──────────┬──────────┘
                                                      │
                                      ┌───────────────┴───────────────┐
                                      │                               │
                                 ┌────▼────┐                    ┌─────▼─────┐
                                 │   Yes   │                    │    No     │
                                 └────┬────┘                    └─────┬─────┘
                                      │                               │
                          ┌───────────▼───────────┐        ┌──────────▼──────────┐
                          │ Return AgentTimeout   │        │ Sleep 100ms, retry  │
                          └───────────────────────┘        └─────────────────────┘
```

### Single Connection Attempt (`try_connect`)

```
┌────────────────────────────────────────────────────────────────────────┐
│ 1. UnixStream::connect(vsock_path)                                     │
│    - Connect to Firecracker's vsock Unix Domain Socket                │
│    - Path format: /tmp/bouvet/{vm-id}/v.sock                          │
└────────────────────────────────────┬───────────────────────────────────┘
                                     │
                                     ▼
┌────────────────────────────────────────────────────────────────────────┐
│ 2. Split stream into read/write halves                                │
│    - tokio::io::split(stream)                                         │
│    - Wrap in BufReader and BufWriter                                  │
└────────────────────────────────────┬───────────────────────────────────┘
                                     │
                                     ▼
┌────────────────────────────────────────────────────────────────────────┐
│ 3. Send CONNECT handshake                                              │
│    - Write: "CONNECT 52\n"                                             │
│    - Flush writer                                                      │
└────────────────────────────────────┬───────────────────────────────────┘
                                     │
                                     ▼
┌────────────────────────────────────────────────────────────────────────┐
│ 4. Read handshake response                                             │
│    - Expect: "OK 52\n"                                                 │
│    - Any other response → Connection error                            │
└────────────────────────────────────┴───────────────────────────────────┘
```

### CONNECT Handshake Protocol

This handshake is required by Firecracker's vsock multiplexer:

```
Host → Guest:  CONNECT 52\n
Guest → Host:  OK 52\n
```

After successful handshake, the connection is ready for JSON-RPC communication.

---

## RPC Call Flow

### Public API

```rust
pub async fn call<P: Serialize, R: DeserializeOwned>(
    &mut self,
    method: &str,
    params: P,
) -> Result<R, CoreError>
```

### Call Execution Steps

```
┌─────────────────────────────────────────────────────────────────────────┐
│ 1. Generate request ID                                                  │
│    - Use next_id, then increment                                        │
└─────────────────────────────────────┬───────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 2. Build JSON-RPC 2.0 request                                           │
│    {                                                                    │
│      "jsonrpc": "2.0",                                                  │
│      "id": <next_id>,                                                   │
│      "method": "<method>",                                              │
│      "params": <params>                                                 │
│    }                                                                    │
└─────────────────────────────────────┬───────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 3. Serialize and send                                                   │
│    - serde_json::to_string(&request)                                    │
│    - Write to socket + newline delimiter                               │
│    - Flush writer                                                       │
└─────────────────────────────────────┬───────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 4. Read response with timeout (30s)                                     │
│    - tokio::time::timeout(RPC_TIMEOUT, reader.read_line())             │
│    - On timeout → Return CoreError::Rpc { code: -1, message: "..." }   │
│    - On IO error → Return error                                        │
└─────────────────────────────────────┬───────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 5. Parse JSON response                                                  │
│    - serde_json::from_str()                                             │
└─────────────────────────────────────┬───────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 6. Check for error field                                                │
│    - If present → Return CoreError::Rpc { code, message }              │
└─────────────────────────────────────┬───────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 7. Extract and deserialize result                                       │
│    - Get "result" field                                                │
│    - Deserialize to type R                                              │
│    - Return Ok(result)                                                  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Timeout Handling

| Timeout Type | Duration | Behavior |
|--------------|----------|----------|
| Connection timeout | 10s | Retry loop exhausted → `CoreError::AgentTimeout(10s)` |
| RPC response timeout | 30s | `tokio::time::timeout` wrapping read → `CoreError::Rpc { code: -1, message: "response timeout" }` |

> [!NOTE]
> The RPC timeout returns a `CoreError::Rpc` variant (with code -1) rather than `CoreError::AgentTimeout`. The `AgentTimeout` variant is only used for connection establishment failures.

---

## Error Mapping

| Error Source | CoreError Variant | Description |
|--------------|-------------------|-------------|
| Socket connect failure | `Connection(String)` | "socket connect failed: {io_error}" |
| Handshake write failure | `Connection(String)` | "handshake write failed: {io_error}" |
| Handshake rejected | `Connection(String)` | "handshake failed: {response}" |
| Connection retry exhausted | `AgentTimeout(Duration)` | Agent not reachable within 10s |
| RPC response timeout | `Rpc { code: -1, message }` | Response not received within 30s |
| Agent returns error | `Rpc { code, message }` | JSON-RPC error from agent |
| JSON parse failure | `Json(serde_json::Error)` | Request serialization or response parsing |
| IO read/write error | `Io(std::io::Error)` | Socket read/write failures |
| Missing result field | `Rpc { code: -1, message }` | "missing result in response" |

---

## High-Level Methods

The `AgentClient` provides convenient wrapper methods for common operations:

### `ping()`

```rust
pub async fn ping(&mut self) -> Result<(), CoreError>
```

Health check. Calls `ping` RPC method, expects `{ "pong": true }` response.

### `exec(cmd)`

```rust
pub async fn exec(&mut self, cmd: &str) -> Result<ExecResult, CoreError>
```

Execute a shell command. Params: `{ "cmd": "<command>" }`.

### `exec_code(lang, code)`

```rust
pub async fn exec_code(&mut self, lang: &str, code: &str) -> Result<ExecResult, CoreError>
```

Execute code in a specific language. Params: `{ "lang": "<lang>", "code": "<code>" }`.

Supported languages: `python`, `python3`, `node`, `javascript`, `bash`, `sh`.

### `read_file(path)`

```rust
pub async fn read_file(&mut self, path: &str) -> Result<String, CoreError>
```

Read file contents from guest. Params: `{ "path": "<path>" }`.

### `write_file(path, content)`

```rust
pub async fn write_file(&mut self, path: &str, content: &str) -> Result<(), CoreError>
```

Write content to a file on guest. Params: `{ "path": "<path>", "content": "<content>" }`.

### `list_dir(path)`

```rust
pub async fn list_dir(&mut self, path: &str) -> Result<Vec<FileEntry>, CoreError>
```

List directory contents. Params: `{ "path": "<path>" }`.

---

## Response Types

### ExecResult

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    pub exit_code: i32,  // -1 if process couldn't start
    pub stdout: String,
    pub stderr: String,
}

impl ExecResult {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}
```

### FileEntry

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,      // File or directory name
    pub is_dir: bool,      // True if directory
    pub size: u64,         // Size in bytes (0 for directories)
}
```

---

## Thread Safety

The `AgentClient` is **not** thread-safe on its own:
- Uses exclusive mutable borrows (`&mut self`) for all operations
- In practice, wrapped in `Arc<Mutex<AgentClient>>` within `Sandbox` struct:

```rust
// In sandbox.rs
pub struct Sandbox {
    // ...
    client: Arc<Mutex<AgentClient>>,
    // ...
}
```

This design allows concurrent access to different sandboxes while serializing operations within a single sandbox.

---

## Usage Example

```rust
use bouvet_core::client::AgentClient;
use std::path::Path;

async fn example() -> Result<(), bouvet_core::CoreError> {
    // Connect to agent (retries automatically for up to 10s)
    let mut client = AgentClient::connect(Path::new("/tmp/bouvet/vm-abc/v.sock")).await?;
    
    // Health check
    client.ping().await?;
    
    // Execute command
    let result = client.exec("echo 'Hello, World!'").await?;
    println!("Output: {}", result.stdout);
    
    // Execute Python code
    let code_result = client.exec_code("python", "print(2 + 2)").await?;
    assert!(code_result.success());
    
    // File operations
    client.write_file("/tmp/test.txt", "content").await?;
    let content = client.read_file("/tmp/test.txt").await?;
    
    // List directory
    let entries = client.list_dir("/tmp").await?;
    for entry in entries {
        println!("{}: {} bytes", entry.name, entry.size);
    }
    
    Ok(())
}
```

---

## Tracing

The client uses the `tracing` crate for structured logging:

| Level | Log Point | Content |
|-------|-----------|---------|
| `debug` | Connection start | Path being connected to |
| `info` | Connection success | Elapsed time, attempt count |
| `warn` | Connection timeout | Elapsed time, attempt count |
| `trace` | Connection attempt | Individual retry attempts |
| `debug` | RPC request | Method name and ID |
| `trace` | RPC request body | Full JSON request |
| `trace` | RPC response body | Full JSON response |
| `debug` | RPC success | Method name and ID |
| `debug` | RPC error | Error code and message |
| `warn` | RPC timeout | Method, ID, timeout duration |
| `debug` | High-level ops | Command, path, code length |

---

## Related Documentation

- [vsock Communication](./VSOCK_COMMUNICATION.md) — Details on the vsock transport layer
- [Agent Protocol](./AGENT_PROTOCOL.md) — JSON-RPC 2.0 protocol specification
- [Agent Internals](./AGENT_INTERNALS.md) — Guest-side implementation
- [Sandbox Lifecycle](./SANDBOX_LIFECYCLE.md) — How sandboxes manage agent clients
