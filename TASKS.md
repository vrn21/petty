# Architecture Documentation Gap Analysis — Layer-Ordered Work Items

> **Purpose**: Each section below is a self-contained work item for an agent to create detailed architecture documentation. Sections are ordered by architectural layer depth (bottom → top).

---

## Layer Reference

```
Layer 5 (Top)     │  bouvet-mcp      │  MCP Server
Layer 4           │  bouvet-core     │  Orchestration
Layer 3           │  bouvet-agent    │  Guest Daemon
Layer 2           │  bouvet-vm       │  VM Wrapper
Layer 1 (Bottom)  │  Firecracker     │  Hypervisor
```

---

# WORK ITEM 1: Rootfs Image Architecture

**Layer**: 1.5 (Between Firecracker and VM — the disk image)  
**Target File**: `docs/internals/ROOTFS_IMAGE.md`  
**Related Code**: `images/` directory, Dockerfile rootfs build

## Scope

Document the ext4 rootfs image that boots inside Firecracker.

## Required Content

### 1.1 Base Image
- Debian Bullseye minimal base
- Size constraints and optimization
- ext4 filesystem layout

### 1.2 Pre-installed Runtimes
| Runtime | Version | Binary Path | Purpose |
|---------|---------|-------------|---------|
| Python | 3.11 | [/usr/bin/python3](file:///usr/bin/python3) | Code execution |
| Node.js | 20.x | `/usr/bin/node` | Code execution |
| Bash | 5.x | [/bin/bash](file:///bin/bash) | Shell commands |
| rustc | stable | `/usr/bin/rustc` | Rust compilation |

### 1.3 Agent Installation
- Binary location: `/usr/bin/bouvet-agent`
- Service file: `/etc/systemd/system/bouvet-agent.service`
- Service type: `simple` (no forking)
- Restart policy

### 1.4 Boot Sequence
```
1. Firecracker loads vmlinux kernel
2. Kernel mounts rootfs.ext4
3. systemd starts as PID 1
4. bouvet-agent.service starts
5. Agent binds vsock port 52
6. Agent ready for host connections
```

### 1.5 Build Process
- How Dockerfile builds the rootfs
- Cross-compilation for aarch64/x86_64
- Image size optimization techniques

---

# WORK ITEM 2: VM Layer Deep Dive

**Layer**: 2 (bouvet-vm)  
**Target File**: `docs/internals/VM_LAYER.md`  
**Related Code**: `crates/bouvet-vm/src/` (builder.rs, machine.rs, config.rs, vsock.rs)

## Scope

Document the `bouvet-vm` crate — the Firecracker wrapper.

## Required Content

### 2.1 Architecture
```
┌──────────────────────────────────────────┐
│                bouvet-vm                  │
├──────────────────────────────────────────┤
│  VmBuilder → MachineConfig → VirtualMachine │
└──────────────────────────────────────────┘
         │
         ▼
┌──────────────────────────────────────────┐
│           firepilot SDK                   │
│  (Firecracker API client)                │
└──────────────────────────────────────────┘
```

### 2.2 Key Types

| Type | File | Purpose |
|------|------|---------|
| `VmBuilder` | builder.rs | Fluent configuration API |
| `MachineConfig` | config.rs | Validated VM configuration |
| `VirtualMachine` | machine.rs | Running VM instance |
| `VsockConfig` | vsock.rs | vsock socket configuration |
| `VmState` | machine.rs | Creating/Running/Paused/Stopped |

### 2.3 VM Lifecycle
```
VmBuilder::new()
    .kernel(path)
    .rootfs(path)
    .vcpus(n)
    .memory_mib(m)
    .with_vsock(cid)
    .build().await  ─────▶  VirtualMachine { state: Running }
                                    │
                            start() / stop() / kill()
                                    │
                            destroy() ─────▶  Resources cleaned up
```

### 2.4 Firecracker API Integration
- Socket path: `/tmp/bouvet/{vm-id}/firecracker.sock`
- Configuration sequence (boot_source, drives, vsock)
- start_machine() call

### 2.5 Error Types
- `VmError::Creation` — Firecracker spawn failures
- `VmError::State` — Invalid state transitions
- `VmError::Configuration` — Invalid config

---

# WORK ITEM 3: vsock Communication

**Layer**: 2-3 (Bridge between VM and Agent)  
**Target File**: `docs/internals/VSOCK_COMMUNICATION.md`  
**Related Code**: `bouvet-vm/src/vsock.rs`, `bouvet-agent/src/main.rs`, `bouvet-core/src/client.rs`

## Scope

Document vsock as the host-guest communication channel.

## Required Content

### 3.1 Why vsock?
| Transport | Latency | Setup | Security |
|-----------|---------|-------|----------|
| vsock (AF_VSOCK) | <1ms | Zero config | Isolated channel |
| TAP/network | ~5ms | IP/routing config | Network exposure |
| Serial port | ~50ms | Minimal | Very slow |

### 3.2 CID (Context ID) Assignment
- CID 0, 1, 2: Reserved by vsock spec
- CID 3+: Available for VMs
- `AtomicU32` counter in pool for unique CIDs
- Port: Fixed at 52 (guest listens)

### 3.3 Socket Paths
- Host side: `/tmp/bouvet/{vm-id}/v.sock` (Unix Domain Socket)
- Guest side: vsock port 52 (AF_VSOCK)

### 3.4 Connection Flow
```
┌────────────────┐                    ┌────────────────┐
│      Host      │                    │     Guest      │
│  AgentClient   │                    │  bouvet-agent  │
└───────┬────────┘                    └───────┬────────┘
        │                                      │
        │ 1. connect(v.sock)                   │
        │ ─────────────────────────────────▶   │
        │                                      │
        │ 2. "CONNECT 52\n"                    │
        │ ─────────────────────────────────▶   │
        │                                      │
        │ 3. "OK 52\n"                         │
        │ ◀─────────────────────────────────   │
        │                                      │
        │ 4. JSON-RPC over newline-delimited   │
        │ ◀────────────────────────────────▶   │
```

### 3.5 Firecracker vsock Proxy
- Firecracker bridges UDS (host) ↔ AF_VSOCK (guest)
- CONNECT handshake required before RPC
- No authentication (isolated by hypervisor)

---

# WORK ITEM 4: Agent Protocol Specification

**Layer**: 3 (bouvet-agent)  
**Target File**: `docs/internals/AGENT_PROTOCOL.md`  
**Related Code**: `crates/bouvet-agent/src/protocol.rs`, `handler.rs`

## Scope

Document the JSON-RPC 2.0 protocol used for guest-host communication.

## Required Content

### 4.1 Wire Format
Newline-delimited JSON-RPC 2.0:
```
{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}\n
```

### 4.2 Request Schema
```json
{
  "jsonrpc": "2.0",        // Required, must be "2.0"
  "id": 1,                  // Required, u64 identifier
  "method": "ping",         // Required, method name
  "params": {}              // Optional, object or array
}
```

### 4.3 Response Schema
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {...}          // On success
}
// OR
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32601,
    "message": "method not found"
  }
}
```

### 4.4 Methods Reference

| Method | Params | Result | Description |
|--------|--------|--------|-------------|
| `ping` | `{}` | `{pong: true}` | Health check |
| `exec` | `{cmd: string}` | `ExecResult` | Shell command |
| `exec_code` | `{lang: string, code: string}` | `ExecResult` | Code execution |
| `read_file` | `{path: string}` | `{content: string}` | Read file |
| `write_file` | `{path: string, content: string}` | `{success: bool}` | Write file |
| `list_dir` | `{path: string}` | `{entries: FileEntry[]}` | List directory |

### 4.5 ExecResult Type
```json
{
  "exit_code": 0,    // i32, -1 if spawn failed
  "stdout": "...",   // string
  "stderr": "..."    // string
}
```

### 4.6 FileEntry Type
```json
{
  "name": "file.txt",   // string
  "is_dir": false,      // boolean
  "size": 1024          // u64 bytes
}
```

### 4.7 Error Codes
| Code | Name | Description |
|------|------|-------------|
| -32700 | Parse error | Invalid JSON |
| -32600 | Invalid request | Not a valid request object |
| -32601 | Method not found | Unknown method |
| -32602 | Invalid params | Wrong params |
| -32603 | Internal error | Server error |

### 4.8 Language Mapping
| Input | Resolved Binary | Notes |
|-------|-----------------|-------|
| `python`, `python3` | `/usr/bin/python3` | Python 3.11 |
| `node`, `javascript` | `/usr/bin/node` | Node.js 20 |
| `bash`, `sh`, `shell` | `/bin/bash` | Bash 5.x |

---

# WORK ITEM 5: Agent Internals

**Layer**: 3 (bouvet-agent implementation)  
**Target File**: `docs/internals/AGENT_INTERNALS.md`  
**Related Code**: `crates/bouvet-agent/src/` (main.rs, exec.rs, fs.rs, handler.rs)

## Scope

Document how the guest agent is implemented.

## Required Content

### 5.1 Module Structure
```
bouvet-agent/src/
├── main.rs      # Entry point, vsock listener
├── protocol.rs  # JSON-RPC types
├── handler.rs   # Request routing
├── exec.rs      # Command/code execution
└── fs.rs        # File operations
```

### 5.2 Runtime Configuration
- Tokio `current_thread` runtime (musl compatibility)
- No multi-threading (simpler on minimal guest)
- Tracing to stderr

### 5.3 vsock Listener
```rust
VsockListener::bind(VsockAddr::new(VMADDR_CID_ANY, 52))
```
- Listens on port 52
- Accepts from any CID (host connects as CID 2)

### 5.4 Request Processing
```
read_line() → parse JSON → handle_request() → serialize → write_line()
```

### 5.5 Code Execution (exec.rs)
- Write code to `/tmp/script.{ext}`
- Spawn process with appropriate interpreter
- Capture stdout/stderr
- Return exit code

### 5.6 File Operations (fs.rs)
- `read_file`: std::fs::read_to_string
- `write_file`: std::fs::write (creates parent dirs)
- `list_dir`: std::fs::read_dir with metadata

---

# WORK ITEM 6: Sandbox Lifecycle

**Layer**: 4 (bouvet-core)  
**Target File**: `docs/internals/SANDBOX_LIFECYCLE.md`  
**Related Code**: `crates/bouvet-core/src/sandbox.rs`, `manager.rs`

## Scope

Document the sandbox abstraction and its state machine.

## Required Content

### 6.1 Sandbox State Machine
```
         create()
            │
            ▼
     ┌──────────────┐
     │   Creating   │──▶ VM booting, Firecracker starting
     └──────┬───────┘
            │
            │ agent.ping() succeeds
            ▼
     ┌──────────────┐
     │    Ready     │◀───────────────┐
     └──────┬───────┘                │
            │                        │
            │ execute(), read_file() │
            ▼                        │
     ┌──────────────┐                │
     │   Working    │────────────────┘
     └──────┬───────┘
            │
            │ destroy()
            ▼
     ┌──────────────┐
     │  Destroyed   │
     └──────────────┘
```

### 6.2 SandboxId
- Type: `Uuid` (v4)
- Matches underlying VM ID
- Used as key in SandboxManager

### 6.3 Sandbox Components
```
Sandbox {
    id: SandboxId,
    vm: VirtualMachine,      // From bouvet-vm
    agent: Mutex<AgentClient>, // vsock connection
    state: SandboxState,
}
```

### 6.4 Health Checking
- `is_healthy()`: Attempts ping, returns bool
- Used during pool acquisition
- Unhealthy sandboxes are destroyed

### 6.5 Timeout Handling
- Agent connection: 10s retry loop
- RPC timeout: 30s per call
- Timeout → destroy sandbox

---

# WORK ITEM 7: AgentClient Communication

**Layer**: 4 (bouvet-core)  
**Target File**: `docs/internals/AGENT_CLIENT.md`  
**Related Code**: `crates/bouvet-core/src/client.rs`

## Scope

Document the host-side client for agent communication.

## Required Content

### 7.1 Connection Establishment
```rust
AgentClient::connect(vsock_path: &Path) -> Result<Self, CoreError>
```
- Retry loop: 100ms interval, 10s total timeout
- CONNECT handshake on first connection
- Returns ready client

### 7.2 RPC Call Flow
```rust
fn call<P, R>(&mut self, method: &str, params: P) -> Result<R, CoreError>
```
1. Serialize request to JSON
2. Write line to socket
3. Read response line
4. Deserialize and extract result/error

### 7.3 Timeout Handling
- RPC timeout: 30 seconds
- tokio::time::timeout wrapping read
- CoreError::AgentTimeout on expiry

### 7.4 Error Mapping
| Agent Error | CoreError Variant |
|-------------|-------------------|
| code: -32xxx | CoreError::Rpc |
| IO failure | CoreError::Io |
| Timeout | CoreError::AgentTimeout |
| JSON parse | CoreError::Json |

---

# WORK ITEM 8: Warm Pool Architecture

**Layer**: 4 (bouvet-core)  
**Target File**: `docs/internals/WARM_POOL.md`  
**Related Code**: `crates/bouvet-core/src/pool.rs` (528 lines)

## Scope

Document the sandbox warm pool for latency reduction.

## Required Content

### 8.1 Purpose
- Cold start: ~500ms (VM boot + agent ready)
- Warm pool: ~150ms (pre-booted sandbox)
- Achieved via pre-provisioned sandbox queue

### 8.2 Architecture
```
┌─────────────────────────────────────────────────────┐
│                   SandboxPool                        │
├─────────────────────────────────────────────────────┤
│  pool: Arc<Mutex<VecDeque<Sandbox>>>                │
│  config: PoolConfig                                  │
│  stats: Arc<PoolStats>                               │
│  shutdown: Arc<AtomicBool>                          │
│  filler_handle: Option<JoinHandle>                  │
└─────────────────────────────────────────────────────┘
```

### 8.3 Configuration (PoolConfig)
| Field | Default | Description |
|-------|---------|-------------|
| `min_size` | 3 | Target pool size |
| `max_boots` | 2 | Max concurrent boot operations |
| `fill_interval` | 500ms | Check frequency |
| `manager_config` | - | VM configuration template |

### 8.4 Background Filler Task
```
loop {
    wait(fill_interval)
    if shutdown { break }
    
    current_size = pool.lock().len()
    if current_size < min_size {
        boots_needed = min_size - current_size
        for _ in 0..boots_needed {
            semaphore.acquire()  // Limit concurrency
            spawn(create_sandbox_and_add_to_pool)
        }
    }
}
```

### 8.5 Acquisition Flow
```rust
pub async fn acquire(&self) -> Result<Sandbox, CoreError>
```
1. Lock pool
2. Pop front (warm sandbox)
3. If empty → cold-start fallback
4. Health check (ping)
5. If unhealthy → discard, retry
6. Return healthy sandbox
7. Update stats (warm_hits/cold_misses)

### 8.6 Statistics (PoolStats)
| Metric | Type | Description |
|--------|------|-------------|
| `warm_hits` | AtomicU64 | Pool served requests |
| `cold_misses` | AtomicU64 | Fallback creates |
| `created` | AtomicU64 | Total sandboxes created |
| `destroyed` | AtomicU64 | Total sandboxes destroyed |

### 8.7 Shutdown Sequence
1. Set shutdown flag
2. Notify filler (Notify::notify_one)
3. Wait for filler handle
4. Drain pool, destroy each sandbox

---

# WORK ITEM 9: SandboxManager

**Layer**: 4 (bouvet-core)  
**Target File**: `docs/internals/SANDBOX_MANAGER.md`  
**Related Code**: `crates/bouvet-core/src/manager.rs`

## Scope

Document the central sandbox registry.

## Required Content

### 9.1 Purpose
- Central registry of all active sandboxes
- Thread-safe concurrent access
- Lifecycle management (create/get/destroy)

### 9.2 Architecture
```rust
pub struct SandboxManager {
    sandboxes: DashMap<SandboxId, Sandbox>,
    config: ManagerConfig,
    cid_counter: AtomicU32,
}
```

### 9.3 ManagerConfig
| Field | Description |
|-------|-------------|
| `kernel_path` | Path to vmlinux |
| `rootfs_path` | Path to rootfs.ext4 |
| `firecracker_path` | Path to firecracker binary |
| `chroot_path` | Working directory for VMs |

### 9.4 CID Counter
- Starts at 3 (0,1,2 reserved)
- Incremented atomically per create
- No reuse (simple, no CID exhaustion in practice)

### 9.5 Operations
| Method | Description |
|--------|-------------|
| `create(config)` | Create new sandbox, add to map |
| `get(id)` | Get sandbox reference |
| `destroy(id)` | Remove from map, cleanup |
| `destroy_all()` | Cleanup all sandboxes (shutdown) |
| `list()` | List all sandbox IDs |

### 9.6 Thread Safety
- DashMap: Lock-free concurrent hashmap
- Per-sandbox Mutex for agent client
- No global lock contention

---

# WORK ITEM 10: Error Handling Strategy

**Layer**: 2-4 (Cross-cutting)  
**Target File**: `docs/internals/ERROR_HANDLING.md`  
**Related Code**: `bouvet-vm/src/error.rs`, `bouvet-core/src/error.rs`

## Scope

Document error types and propagation across layers.

## Required Content

### 10.1 Error Hierarchy
```
┌──────────────────────────────────────────┐
│ MCP Layer (bouvet-mcp)                   │
│ CallToolResult { isError: true }         │
└──────────────────┬───────────────────────┘
                   │ mapped from
┌──────────────────▼───────────────────────┐
│ Core Layer (bouvet-core)                 │
│ CoreError enum                            │
└──────────────────┬───────────────────────┘
                   │ wraps
┌──────────────────▼───────────────────────┐
│ VM Layer (bouvet-vm)                     │
│ VmError enum                              │
└──────────────────────────────────────────┘
```

### 10.2 VmError Variants
| Variant | Description |
|---------|-------------|
| `Creation` | Firecracker spawn failed |
| `State` | Invalid state transition |
| `Configuration` | Bad config values |
| `Network` | Network setup failed |

### 10.3 CoreError Variants
| Variant | Cause |
|---------|-------|
| `Vm(VmError)` | Wrapped VM error |
| `Connection(String)` | vsock connect failed |
| `AgentTimeout(Duration)` | RPC timeout |
| `Rpc { code, message }` | Agent error response |
| `NotFound(SandboxId)` | Unknown sandbox |
| `InvalidState` | Wrong sandbox state |
| `Json(serde_json::Error)` | Serialization failed |
| `Io(std::io::Error)` | IO operation failed |

### 10.4 MCP Error Mapping
```rust
// In server.rs
match result {
    Ok(data) => CallToolResult::success(json!(data)),
    Err(e) => CallToolResult::error(format!("{}", e)),
}
```

### 10.5 Retry Semantics
| Operation | Retry? | Timeout |
|-----------|--------|---------|
| Agent connect | Yes, 100ms interval | 10s total |
| RPC call | No | 30s |
| Pool health check | No, discard on fail | — |

---

# WORK ITEM 11: MCP Server Layer

**Layer**: 5 (bouvet-mcp)  
**Target File**: `docs/internals/MCP_SERVER.md`  
**Related Code**: `crates/bouvet-mcp/src/server.rs`, `http.rs`, `config.rs`

## Scope

Document the MCP server implementation.

## Required Content

### 11.1 Architecture
```
┌──────────────────────────────────────────────────────┐
│                    BouvetServer                       │
├──────────────────────────────────────────────────────┤
│  manager: Arc<SandboxManager>                         │
│  pool: Arc<RwLock<Option<SandboxPool>>>              │
│  config: BouvetConfig                                 │
└───────────────────┬──────────────────────────────────┘
                    │
        ┌───────────┴───────────┐
        ▼                       ▼
┌───────────────┐       ┌───────────────┐
│    stdio      │       │     HTTP      │
│  transport    │       │  transport    │
│ (rmcp crate)  │       │ (axum + SSE)  │
└───────────────┘       └───────────────┘
```

### 11.2 BouvetConfig
| Field | Env Var | Default |
|-------|---------|---------|
| `kernel_path` | `BOUVET_KERNEL` | `/var/lib/bouvet/vmlinux` |
| `rootfs_path` | `BOUVET_ROOTFS` | `/var/lib/bouvet/debian-devbox.ext4` |
| `firecracker_path` | `BOUVET_FIRECRACKER` | `/usr/local/bin/firecracker` |
| `chroot_path` | `BOUVET_CHROOT` | `/tmp/bouvet` |
| `transport_mode` | `BOUVET_TRANSPORT` | `both` |
| `pool_enabled` | `BOUVET_POOL_ENABLED` | `true` |
| `pool_min_size` | `BOUVET_POOL_MIN_SIZE` | `3` |

### 11.3 Tool Implementations
| Tool | Handler Method | Pool Aware? |
|------|----------------|-------------|
| `create_sandbox` | `handle_create_sandbox` | Yes (acquire from pool) |
| `destroy_sandbox` | `handle_destroy_sandbox` | No |
| `list_sandboxes` | `handle_list_sandboxes` | No |
| `execute_code` | `handle_execute_code` | No |
| `run_command` | `handle_run_command` | No |
| `read_file` | `handle_read_file` | No |
| `write_file` | `handle_write_file` | No |
| `list_directory` | `handle_list_directory` | No |

### 11.4 HTTP Transport
- Endpoint: `POST /mcp` (JSON-RPC)
- Endpoint: `GET /mcp` (SSE stream)
- Endpoint: `GET /health` (health check)
- CORS enabled for remote agents

### 11.5 Startup Sequence
1. Load config from env
2. Create SandboxManager
3. Create SandboxPool (if enabled)
4. Start pool filler task
5. Spawn transport tasks (stdio/HTTP)
6. Wait for Ctrl+C
7. Shutdown pool, destroy all sandboxes

---

# WORK ITEM 12: Concurrency Model

**Layer**: Cross-cutting (all layers)  
**Target File**: `docs/internals/CONCURRENCY.md`  
**Related Code**: All crates

## Scope

Document threading and locking patterns.

## Required Content

### 12.1 Async Runtime
| Component | Runtime | Reason |
|-----------|---------|--------|
| bouvet-mcp | Multi-thread | Handle concurrent requests |
| bouvet-core | Multi-thread | Concurrent sandbox ops |
| bouvet-agent | Single-thread | musl compatibility |

### 12.2 Locking Strategy
| Resource | Lock Type | Granularity |
|----------|-----------|-------------|
| Sandbox registry | DashMap | Per-entry |
| Pool queue | Tokio Mutex | Global |
| Agent client | Tokio Mutex | Per-sandbox |
| Pool reference | RwLock | Server-wide |

### 12.3 Semaphore Usage
- Pool boot concurrency: `Semaphore::new(max_boots)`
- Prevents resource exhaustion during fill

### 12.4 Shutdown Coordination
- `AtomicBool` for shutdown flag
- `Notify` for waking filler task
- `broadcast::channel` for transport shutdown

---

# WORK ITEM 13: Deployment Topology

**Layer**: Infrastructure  
**Target File**: `docs/internals/DEPLOYMENT.md`  
**Related Code**: `Dockerfile`, `terraform/`

## Scope

Document deployment modes and requirements.

## Required Content

### 13.1 Docker Mode
```
┌─────────────────────────────────────────────────────┐
│                 Docker Container                     │
│  ┌───────────────────────────────────────────────┐  │
│  │            bouvet-mcp process                  │  │
│  └───────────────────────────────────────────────┘  │
│           │              │              │            │
│  ┌────────▼────┐ ┌───────▼────┐ ┌───────▼────┐     │
│  │ Firecracker │ │ Firecracker│ │ Firecracker│     │
│  │   VM 1      │ │   VM 2     │ │   VM n     │     │
│  └─────────────┘ └────────────┘ └────────────┘     │
│                                                      │
│  Required: --privileged OR --device=/dev/kvm        │
└─────────────────────────────────────────────────────┘
```

### 13.2 Host Requirements
- Linux kernel 4.14+ with KVM
- vsock module loaded (`vhost_vsock`)
- x86_64 or aarch64

### 13.3 AWS Deployment
- Instance type: c5.metal (bare-metal for KVM)
- AMI: Amazon Linux 2 or Ubuntu
- User data: Docker install, container start

### 13.4 Required Mounts (Docker)
| Mount | Purpose |
|-------|---------|
| `/dev/kvm` | KVM device access |
| `/tmp/bouvet` | VM working directory |

### 13.5 Port Exposure
- 8080: HTTP/SSE transport
- No ports needed for stdio mode

---

## Summary Checklist

| # | Work Item | Layer | Priority |
|---|-----------|-------|----------|
| 1 | Rootfs Image Architecture | 1.5 | Medium |
| 2 | VM Layer Deep Dive | 2 | Medium |
| 3 | vsock Communication | 2-3 | High |
| 4 | Agent Protocol Specification | 3 | Critical |
| 5 | Agent Internals | 3 | Medium |
| 6 | Sandbox Lifecycle | 4 | High |
| 7 | AgentClient Communication | 4 | High |
| 8 | Warm Pool Architecture | 4 | Critical |
| 9 | SandboxManager | 4 | Medium |
| 10 | Error Handling Strategy | 2-4 | High |
| 11 | MCP Server Layer | 5 | Medium |
| 12 | Concurrency Model | All | Medium |
| 13 | Deployment Topology | Infra | Low |
