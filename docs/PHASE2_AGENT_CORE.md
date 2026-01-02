# Phase 2: petty-agent

> Guest agent binary running inside the microVM.

---

## Purpose

Lightweight Rust binary that:

1. Listens on vsock port 52
2. Receives JSON-RPC requests from host
3. Executes commands/code
4. Returns results (exit code, stdout, stderr)

---

## Protocol

**Transport**: vsock port 52  
**Format**: JSON-RPC 2.0 (newline-delimited)

### Request

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "exec",
  "params": { "cmd": "python3 -c \"print(2+2)\"" }
}
```

### Response

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { "exit_code": 0, "stdout": "4\n", "stderr": "" }
}
```

---

## Supported Methods

| Method       | Params                            | Returns                       |
| ------------ | --------------------------------- | ----------------------------- |
| `ping`       | `{}`                              | `{pong: true}`                |
| `exec`       | `{cmd: String}`                   | `{exit_code, stdout, stderr}` |
| `exec_code`  | `{lang: String, code: String}`    | `{exit_code, stdout, stderr}` |
| `read_file`  | `{path: String}`                  | `{content: String}`           |
| `write_file` | `{path: String, content: String}` | `{success: bool}`             |
| `list_dir`   | `{path: String}`                  | `{entries: [...]}`            |

---

## File Structure

```
crates/petty-agent/
├── Cargo.toml
└── src/
    ├── main.rs       # Entry, vsock listener
    ├── protocol.rs   # JSON-RPC types
    ├── handler.rs    # Request routing
    ├── exec.rs       # Command execution
    └── fs.rs         # File operations
```

---

## Implementation Tasks

### Task 1: Create Crate Structure

- Cargo.toml with dependencies
- Empty module files

### Task 2: JSON-RPC Protocol Types

```rust
#[derive(Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Value,
}

#[derive(Serialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Serialize)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}
```

### Task 3: Command Execution (`exec.rs`)

```rust
use std::process::Command;

pub fn exec_command(cmd: &str) -> ExecResult {
    let output = Command::new("sh")
        .args(["-c", cmd])
        .output();

    match output {
        Ok(out) => ExecResult {
            exit_code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into(),
            stderr: String::from_utf8_lossy(&out.stderr).into(),
        },
        Err(e) => ExecResult {
            exit_code: -1,
            stdout: String::new(),
            stderr: e.to_string(),
        },
    }
}
```

### Task 4: Language Execution (`exec_code`)

```rust
pub fn exec_code(lang: &str, code: &str) -> ExecResult {
    let (cmd, args) = match lang {
        "python" | "python3" => ("python3", vec!["-c", code]),
        "node" | "javascript" => ("node", vec!["-e", code]),
        "bash" | "sh" => ("sh", vec!["-c", code]),
        "rust" => return exec_rust(code),  // Special handling
        _ => return ExecResult::error(&format!("unknown language: {}", lang)),
    };

    exec_with_args(cmd, &args)
}
```

### Task 5: File Operations (`fs.rs`)

```rust
pub fn read_file(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

pub fn write_file(path: &str, content: &str) -> Result<bool, String> {
    std::fs::write(path, content).map(|_| true).map_err(|e| e.to_string())
}

pub fn list_dir(path: &str) -> Result<Vec<FileEntry>, String> {
    let entries = std::fs::read_dir(path).map_err(|e| e.to_string())?;
    // ...
}
```

### Task 6: Request Handler (`handler.rs`)

```rust
pub fn handle_request(req: Request) -> Response {
    let result = match req.method.as_str() {
        "ping" => json!({"pong": true}),
        "exec" => {
            let params: ExecParams = serde_json::from_value(req.params)?;
            serde_json::to_value(exec_command(&params.cmd))?
        }
        "exec_code" => {
            let params: ExecCodeParams = serde_json::from_value(req.params)?;
            serde_json::to_value(exec_code(&params.lang, &params.code))?
        }
        // ... other methods
        _ => return Response::error(req.id, "method not found"),
    };

    Response::success(req.id, result)
}
```

### Task 7: Main Entry Point (`main.rs`)

```rust
use tokio::net::UnixListener;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::main]
async fn main() -> Result<()> {
    // In real Firecracker: use vsock
    // For testing: use Unix socket
    let listener = UnixListener::bind("/tmp/petty-agent.sock")?;

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(handle_connection(stream));
    }
}

async fn handle_connection(stream: UnixStream) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let request: Request = serde_json::from_str(&line)?;
        let response = handle_request(request);
        let json = serde_json::to_string(&response)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        line.clear();
    }
}
```

---

## Dependencies

```toml
[package]
name = "petty-agent"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "net", "io-util", "macros"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
```

---

## Build & Test

```bash
# Build for Linux (from macOS)
cargo build -p petty-agent --release --target x86_64-unknown-linux-musl

# Test locally with Unix socket
cargo run -p petty-agent

# Test with netcat
echo '{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}' | nc -U /tmp/petty-agent.sock
```

---

## Acceptance Criteria

- [ ] Compiles to static Linux binary
- [ ] Listens on Unix socket (vsock-compatible)
- [ ] `ping` method works
- [ ] `exec` runs shell commands
- [ ] `exec_code` runs Python/Node/Bash
- [ ] File operations work
- [ ] Proper JSON-RPC error handling
- [ ] No panics on malformed input
