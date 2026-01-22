# vsock Communication

> **Layer**: 2-3 (Bridge between `bouvet-vm` and `bouvet-agent`)  
> **Related Code**: [`bouvet-vm/src/vsock.rs`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-vm/src/vsock.rs), [`bouvet-agent/src/main.rs`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/main.rs), [`bouvet-core/src/client.rs`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-core/src/client.rs)

This document describes **vsock** (Virtual Socket) as the communication channel between the host and guests in Bouvet's microVM architecture.

---

## 1. Why vsock?

Bouvet uses vsock (AF_VSOCK) instead of traditional networking for host-guest communication:

| Transport | Latency | Setup | Security | Use Case |
|-----------|---------|-------|----------|----------|
| **vsock (AF_VSOCK)** | <1ms | Zero config | Isolated channel | ✅ Bouvet |
| TAP/network | ~5ms | IP/routing config | Network exposure | Traditional VMs |
| Serial port | ~50ms | Minimal | Very slow | Debugging |

### Key Benefits

- **Zero network configuration**: No IP addresses, routing, or firewall rules required
- **Low latency**: Direct hypervisor-mediated communication
- **Strong isolation**: No network stack exposure; the channel only exists between host and guest
- **Deterministic addressing**: CID-based addressing is simple and predictable

---

## 2. CID (Context ID) Assignment

vsock uses **Context IDs (CIDs)** to identify endpoints:

| CID | Usage |
|-----|-------|
| `0` | Reserved (hypervisor) |
| `1` | Reserved (local loopback) |
| `2` | Host |
| `3+` | Available for guest VMs |

### CID Assignment in Bouvet

```rust
// In bouvet-core/src/manager.rs
pub struct SandboxManager {
    cid_counter: AtomicU32, // Starts at 3
    // ...
}
```

- CIDs start at 3 and increment atomically for each new VM
- No CID reuse in practice (simple approach, no exhaustion concern)
- Each VM gets a unique CID for its lifetime

### Guest Port

The guest agent listens on a **fixed port**: `52`

```rust
// In bouvet-agent/src/main.rs
const GUEST_PORT: u32 = 52;
```

---

## 3. Socket Paths

Firecracker bridges the host Unix domain socket to the guest vsock:

| Side | Socket Type | Path/Address |
|------|-------------|--------------|
| **Host** | Unix Domain Socket | `/tmp/bouvet/{vm-id}/v.sock` |
| **Guest** | vsock (AF_VSOCK) | Port 52 on `VMADDR_CID_ANY` |

### Host-Side Path Generation

```rust
// In bouvet-vm/src/config.rs
impl VsockConfig {
    pub fn for_vm(cid: u32, chroot_path: &Path, vm_id: &str) -> Self {
        Self {
            guest_cid: cid,
            uds_path: chroot_path.join(vm_id).join("v.sock"),
        }
    }
}
```

**Example paths:**
- Firecracker API socket: `/tmp/bouvet/550e8400-e29b-41d4-a716-446655440000/firecracker.socket`
- vsock UDS: `/tmp/bouvet/550e8400-e29b-41d4-a716-446655440000/v.sock`

---

## 4. Connection Flow

The connection sequence involves a handshake through Firecracker's vsock proxy:

```
┌────────────────┐                    ┌────────────────┐
│      Host      │                    │     Guest      │
│  AgentClient   │                    │  bouvet-agent  │
└───────┬────────┘                    └───────┬────────┘
        │                                      │
        │ 1. connect("/tmp/bouvet/{id}/v.sock")│
        │ ────────────────────────────────────▶│
        │                                      │
        │ 2. "CONNECT 52\n"                    │
        │ ────────────────────────────────────▶│
        │                                      │
        │ 3. "OK 52\n"                         │
        │ ◀────────────────────────────────────│
        │                                      │
        │ 4. JSON-RPC messages (newline-delimited)
        │ ◀───────────────────────────────────▶│
```

### Step-by-Step Breakdown

1. **Host connects to UDS**: The `AgentClient` connects to the Unix domain socket created by Firecracker
2. **CONNECT handshake**: Host sends `CONNECT {port}\n` to specify the guest port
3. **OK response**: Guest agent responds with `OK {port}\n` confirming the connection
4. **RPC communication**: JSON-RPC 2.0 messages are exchanged over the established channel

---

## 5. Host-Side: AgentClient

The [`AgentClient`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-core/src/client.rs) handles the host-side connection:

### Connection with Retry

```rust
// In bouvet-core/src/client.rs
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const RETRY_INTERVAL: Duration = Duration::from_millis(100);

pub async fn connect(vsock_path: &Path) -> Result<Self, CoreError> {
    let mut attempts = 0u32;
    loop {
        attempts += 1;
        match Self::try_connect(vsock_path).await {
            Ok(client) => return Ok(client),
            Err(e) => {
                if start.elapsed() >= CONNECT_TIMEOUT {
                    return Err(CoreError::AgentTimeout(CONNECT_TIMEOUT));
                }
                tokio::time::sleep(RETRY_INTERVAL).await;
            }
        }
    }
}
```

### Handshake Implementation

```rust
async fn try_connect(vsock_path: &Path) -> Result<Self, CoreError> {
    // 1. Connect to Unix socket
    let stream = UnixStream::connect(vsock_path).await?;
    let (read_half, write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    let mut writer = BufWriter::new(write_half);

    // 2. Send CONNECT handshake
    writer.write_all(format!("CONNECT {GUEST_PORT}\n").as_bytes()).await?;
    writer.flush().await?;

    // 3. Read and validate response
    let mut response = String::new();
    reader.read_line(&mut response).await?;
    
    if !response.starts_with("OK ") {
        return Err(CoreError::Connection(format!("handshake failed: {}", response)));
    }

    Ok(Self { reader, writer, next_id: 1 })
}
```

### RPC Call with Timeout

```rust
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn call<P: Serialize, R: DeserializeOwned>(
    &mut self,
    method: &str,
    params: P,
) -> Result<R, CoreError> {
    // Send JSON-RPC request
    let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    self.writer.write_all(serde_json::to_string(&request)?.as_bytes()).await?;
    self.writer.write_all(b"\n").await?;
    self.writer.flush().await?;

    // Read response with timeout
    let mut response_str = String::new();
    match timeout(RPC_TIMEOUT, self.reader.read_line(&mut response_str)).await {
        Ok(Ok(_)) => { /* parse response */ }
        Err(_) => return Err(CoreError::Rpc { code: -1, message: "response timeout".into() }),
        // ...
    }
}
```

---

## 6. Guest-Side: bouvet-agent

The [`bouvet-agent`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/main.rs) runs inside the guest VM:

### vsock Listener Setup

```rust
// In bouvet-agent/src/main.rs
const GUEST_PORT: u32 = 52;

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    // Check vsock device exists
    if !std::path::Path::new("/dev/vsock").exists() {
        return Err("/dev/vsock does not exist".into());
    }

    // Bind to port 52, accept from any CID
    let addr = VsockAddr::new(VMADDR_CID_ANY, GUEST_PORT);
    let listener = VsockListener::bind(addr)?;

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        // peer_addr.cid() == 2 (host)
        handle_connection(stream).await?;
    }
}
```

### CONNECT Handshake Handling

```rust
async fn handle_connection(mut stream: VsockStream) -> Result<()> {
    let (read_half, write_half) = stream.split();
    let mut reader = BufReader::new(read_half);
    let mut writer = BufWriter::new(write_half);

    // Read first line
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    // Check for CONNECT handshake
    if let Some(port_str) = line.trim().strip_prefix("CONNECT ") {
        let port: u32 = port_str.parse().unwrap_or(GUEST_PORT);
        
        // Send OK response
        writer.write_all(format!("OK {}\n", port).as_bytes()).await?;
        writer.flush().await?;
        
        line.clear();
    } else {
        // First line was JSON-RPC request (no handshake)
        // Process it directly...
    }

    // Normal JSON-RPC request loop
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 { break; } // Client disconnected
        
        let request: Request = serde_json::from_str(&line)?;
        let response = handle_request(request);
        
        writer.write_all(serde_json::to_string(&response)?.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }

    Ok(())
}
```

---

## 7. Firecracker vsock Proxy

Firecracker acts as a bridge between the host-side UDS and the guest-side AF_VSOCK:

```
┌─────────────────────────────────────────────────────────────────────┐
│                              Host                                    │
│                                                                      │
│  ┌──────────────┐     ┌───────────────────────────────────────────┐ │
│  │ AgentClient  │────▶│ /tmp/bouvet/{vm-id}/v.sock (UDS)          │ │
│  └──────────────┘     └───────────────────────────────────────────┘ │
│                                         │                            │
│                                         │ Firecracker                │
│                                         │ vsock proxy                │
│                                         ▼                            │
├─────────────────────────────────────────────────────────────────────┤
│                             Guest VM                                 │
│                                                                      │
│  ┌──────────────────────────────────────────────────────┐           │
│  │ AF_VSOCK port 52                                      │           │
│  │ VsockListener::bind(VsockAddr::new(VMADDR_CID_ANY,52))│           │
│  └──────────────────────────────────────────────────────┘           │
│                        │                                             │
│                        ▼                                             │
│  ┌──────────────────────────────────────────────────────┐           │
│  │ bouvet-agent                                          │           │
│  └──────────────────────────────────────────────────────┘           │
└─────────────────────────────────────────────────────────────────────┘
```

### vsock Configuration via Firecracker API

The VM layer configures vsock before starting the VM:

```rust
// In bouvet-vm/src/vsock.rs
pub async fn configure_vsock(socket_path: &Path, config: &VsockConfig) -> Result<()> {
    let vsock = Vsock::new(
        config.guest_cid as i32,
        config.uds_path.to_string_lossy().to_string(),
    );

    // PUT /vsock to Firecracker API socket
    let uri: Uri = Uri::new(socket_path, "/vsock").into();
    let request = Request::builder()
        .method(Method::PUT)
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&vsock)?))
        .build()?;

    let client = Client::unix();
    let response = client.request(request).await?;
    
    // Verify success
    if !response.status().is_success() {
        return Err(VmError::Firepilot("vsock configuration failed"));
    }
    
    Ok(())
}
```

---

## 8. Timeout Configuration

| Operation | Timeout | Retry | Behavior |
|-----------|---------|-------|----------|
| Agent connection | 10s | 100ms interval | Retries until timeout, then `CoreError::AgentTimeout` |
| RPC call | 30s | None | Single attempt, then `CoreError::Rpc` |
| Pool health check | — | None | Fails immediately, sandbox discarded |

---

## 9. Security Model

vsock provides inherent security through isolation:

| Aspect | vsock Behavior |
|--------|----------------|
| **No network exposure** | vsock is not routable or discoverable |
| **Hypervisor-mediated** | All traffic passes through Firecracker |
| **No authentication** | Trusted channel; isolated by design |
| **Single tenant** | Each VM has its own CID; no cross-VM communication |

> [!NOTE]
> Since vsock is isolated per-VM and only accessible from the host, no authentication is required. The security boundary is the hypervisor itself.

---

## 10. Wire Format

Messages are exchanged as **newline-delimited JSON-RPC 2.0**:

```
{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}\n
```

Response:
```
{"jsonrpc":"2.0","id":1,"result":{"pong":true}}\n
```

See [AGENT_PROTOCOL.md](file:///Users/vrn21/Developer/rust/petty/docs/internals/AGENT_PROTOCOL.md) for the complete protocol specification.

---

## 11. Error Handling

### Connection Errors

| Error | Cause | Handling |
|-------|-------|----------|
| `CoreError::Connection` | Socket connect failed, handshake rejected | Retry with backoff |
| `CoreError::AgentTimeout` | Agent not responding within 10s | VM likely not ready; may need rebuild |
| `CoreError::Io` | Read/write failure | Connection broken; reestablish |

### RPC Errors

| Error | Cause | Handling |
|-------|-------|----------|
| `CoreError::Rpc { code, message }` | Agent returned error response | Check error code, may be recoverable |
| Response timeout (30s) | Long-running command or hung agent | Consider killing sandbox |

---

## 12. Debugging Tips

### Checking vsock on the Guest

```bash
# Inside the guest VM
ls -la /dev/vsock    # Should exist
ss -la | grep vsock  # Show vsock listeners
journalctl -u bouvet-agent  # Agent logs
```

### Testing vsock from Host

```bash
# Connect to vsock UDS manually
nc -U /tmp/bouvet/{vm-id}/v.sock

# Send CONNECT handshake
CONNECT 52

# Expected response
OK 52

# Send JSON-RPC ping
{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}

# Expected response
{"jsonrpc":"2.0","id":1,"result":{"pong":true}}
```

### Common Issues

| Issue | Symptom | Resolution |
|-------|---------|------------|
| `/dev/vsock` missing | Agent fails to start | Ensure `vhost_vsock` kernel module is loaded |
| Connection timeout | Sandbox creation hangs | Check agent startup in guest logs |
| Handshake failure | "handshake failed" error | Firecracker vsock misconfigured |
