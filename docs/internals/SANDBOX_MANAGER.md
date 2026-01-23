# SandboxManager — Central Sandbox Registry

> **Layer**: 4 (bouvet-core)  
> **Source**: [`manager.rs`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-core/src/manager.rs)

---

## Purpose

The **SandboxManager** is the central registry for all active sandbox instances. It provides:

- **Centralized Lifecycle Management** — Create, access, and destroy sandboxes through a unified API
- **Thread-Safe Concurrent Access** — Multiple readers can access sandboxes simultaneously; writers get exclusive access
- **Resource Limits** — Configurable maximum sandbox count to prevent resource exhaustion
- **Unique CID Assignment** — Automatic vsock Context ID allocation to prevent collisions

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                       SandboxManager                             │
├─────────────────────────────────────────────────────────────────┤
│  sandboxes: Arc<RwLock<HashMap<SandboxId, Sandbox>>>            │
│  config: ManagerConfig                                           │
│  cid_counter: AtomicU32                                          │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
         ┌────────────────────────────────────────┐
         │            SandboxPool                  │
         │  (Optional: warm pool integration)      │
         │  Calls manager.register() for pooled   │
         │  sandboxes                              │
         └────────────────────────────────────────┘
```

The manager sits at the center of the bouvet-core architecture:

- **Upstream**: Used by `bouvet-mcp` server to handle tool requests
- **Downstream**: Creates `Sandbox` instances which wrap `VirtualMachine` from `bouvet-vm`
- **Sidecar**: `SandboxPool` can register pre-warmed sandboxes via `register()`

---

## ManagerConfig

Configuration for the SandboxManager, typically derived from environment variables at the MCP layer.

| Field | Type | Description |
|-------|------|-------------|
| `kernel_path` | `PathBuf` | Path to vmlinux kernel image |
| `rootfs_path` | `PathBuf` | Path to rootfs.ext4 disk image |
| `firecracker_path` | `PathBuf` | Path to firecracker binary |
| `chroot_path` | `PathBuf` | Working directory for VM sockets and state |
| `max_sandboxes` | `usize` | Maximum concurrent sandboxes (default: 100, 0 = unlimited) |

### Example Configuration

```rust
let config = ManagerConfig::new(
    "/var/lib/bouvet/vmlinux",
    "/var/lib/bouvet/debian-devbox.ext4",
    "/usr/local/bin/firecracker",
    "/tmp/bouvet",
);
// config.max_sandboxes defaults to 100
```

---

## CID (Context ID) Counter

The vsock protocol requires each VM to have a unique Context ID (CID) for addressing:

| CID Range | Usage |
|-----------|-------|
| 0 | Reserved (hypervisor) |
| 1 | Reserved (local loopback) |
| 2 | Reserved (host) |
| 3+ | Available for guest VMs |

### Implementation

```rust
cid_counter: AtomicU32::new(3)  // Start at minimum valid CID
```

The counter:
- **Starts at 3** — First valid CID for guest VMs
- **Atomically incremented** — Thread-safe allocation via `fetch_add(1, Ordering::Relaxed)`
- **No reuse** — CIDs are never recycled (in practice, 4 billion CIDs is more than sufficient)

### CID Assignment Flow

```
create(config) called
     │
     ▼
config.vsock_cid = cid_counter.fetch_add(1)
     │
     ▼
Sandbox::create(config)  ─────► VM gets unique CID
```

---

## Operations

### Core Lifecycle Methods

| Method | Description | Lock Type |
|--------|-------------|-----------|
| `create(config)` | Create a new sandbox with custom configuration | Write |
| `create_default()` | Create sandbox using manager's default paths | Write |
| `register(sandbox)` | Register an externally-created sandbox (from pool) | Write |
| `destroy(id)` | Remove and destroy a single sandbox | Write |
| `destroy_all()` | Destroy all sandboxes (for shutdown) | Write |

### Query Methods

| Method | Description | Lock Type |
|--------|-------------|-----------|
| `list()` | Get all sandbox IDs | Read |
| `count()` | Get number of active sandboxes | Read |
| `exists(id)` | Check if a sandbox exists | Read |
| `config()` | Get manager configuration reference | None (sync) |

### Sandbox Access Methods

| Method | Description | Lock Type |
|--------|-------------|-----------|
| `with_sandbox(id, f)` | Execute sync closure on sandbox | Read |
| `with_sandbox_async(id, f)` | Execute async closure on sandbox | Read |

### Direct Operation Methods

These convenience methods avoid lifetime issues with closures:

| Method | Description |
|--------|-------------|
| `execute(id, command)` | Run shell command in sandbox |
| `execute_code(id, language, code)` | Execute code in specified language |
| `read_file(id, path)` | Read file contents from sandbox |
| `write_file(id, path, content)` | Write file to sandbox |
| `list_dir(id, path)` | List directory contents in sandbox |

---

## Thread Safety

### Locking Strategy

```rust
sandboxes: Arc<RwLock<HashMap<SandboxId, Sandbox>>>
```

The manager uses Tokio's async `RwLock` with the following semantics:

| Operation Type | Lock | Concurrency |
|----------------|------|-------------|
| Read (list, exists, count) | Read lock | Multiple concurrent readers |
| Access (with_sandbox) | Read lock | Multiple concurrent readers |
| Create/Destroy | Write lock | Exclusive access |

### Why RwLock over DashMap?

While `DashMap` offers lock-free concurrent access, `RwLock<HashMap>` was chosen because:

1. **Simpler semantics** — Async operations are easier to reason about
2. **Predictable behavior** — No sharded locking complexity
3. **Sufficient performance** — Sandbox operations (VM creates) are slow enough that lock contention is negligible

### Per-Sandbox Locking

Each `Sandbox` internally holds a `Mutex<AgentClient>` for RPC serialization:

```
┌─────────────────────────────────────────────────────────────┐
│  SandboxManager                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ RwLock<HashMap>                                     │    │
│  │   ┌─────────┐  ┌─────────┐  ┌─────────┐            │    │
│  │   │Sandbox A│  │Sandbox B│  │Sandbox C│    ...     │    │
│  │   └────┬────┘  └────┬────┘  └────┬────┘            │    │
│  └────────│────────────│────────────│─────────────────┘    │
│           │            │            │                        │
│           ▼            ▼            ▼                        │
│       Mutex<AC>    Mutex<AC>    Mutex<AC>                   │
│       (RPC lock)   (RPC lock)   (RPC lock)                  │
└─────────────────────────────────────────────────────────────┘
```

This allows:
- Concurrent operations on **different** sandboxes
- Serialized RPC calls within **the same** sandbox

---

## Create Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                     manager.create(config)                       │
└─────────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              │ 1. Check max_sandboxes limit  │
              │    (returns error if reached) │
              └───────────────┬───────────────┘
                              │
              ┌───────────────┴───────────────┐
              │ 2. Assign unique vsock CID    │
              │    cid_counter.fetch_add(1)   │
              └───────────────┬───────────────┘
                              │
              ┌───────────────┴───────────────┐
              │ 3. Sandbox::create(config)    │
              │    - Spawns Firecracker       │
              │    - Boots VM                 │
              │    - Connects to agent        │
              └───────────────┬───────────────┘
                              │
              ┌───────────────┴───────────────┐
              │ 4. Acquire write lock         │
              │    sandboxes.insert(id, sb)   │
              └───────────────┬───────────────┘
                              │
                              ▼
                    Return SandboxId
```

---

## Register Flow (Pool Integration)

When using the warm pool, sandboxes are pre-created and then registered with the manager:

```rust
pub async fn register(&self, sandbox: Sandbox) -> Result<SandboxId, (CoreError, Sandbox)>
```

Key differences from `create()`:
- **No CID assignment** — Sandbox already has a CID from pool creation
- **Error returns sandbox** — On limit failure, returns the sandbox so caller can handle cleanup
- **Same limit check** — Still enforces max_sandboxes

```
┌──────────────────────────────────────────────────────────────┐
│                 Pool acquire() flow                          │
├──────────────────────────────────────────────────────────────┤
│  1. pool.acquire() returns pre-warmed Sandbox                │
│  2. manager.register(sandbox) registers it                   │
│  3. If limit reached, error includes sandbox for cleanup     │
└──────────────────────────────────────────────────────────────┘
```

---

## Destroy Flow

```rust
pub async fn destroy(&self, id: SandboxId) -> Result<(), CoreError>
```

Steps:
1. Acquire write lock
2. Remove sandbox from HashMap → Returns `NotFound` if missing
3. Release write lock (drop early to minimize lock duration)
4. Call `sandbox.destroy()` — Kills Firecracker, cleans up vsock socket

```
manager.destroy(id)
        │
        ▼
    ┌────────────────────┐
    │ Write lock HashMap │
    └─────────┬──────────┘
              │
    ┌─────────▼────────────┐
    │ sandboxes.remove(id) │───── Not found ────► Err(NotFound)
    └─────────┬────────────┘
              │ Found
              ▼
    ┌────────────────────┐
    │ Drop write lock    │
    └─────────┬──────────┘
              │
    ┌─────────▼────────────────┐
    │ sandbox.destroy().await  │
    │  - Stop Firecracker      │
    │  - Remove socket files   │
    └──────────────────────────┘
```

### Destroy All (Shutdown)

```rust
pub async fn destroy_all(&self) -> Result<(), CoreError>
```

Used during graceful shutdown:
1. Take ownership of all sandboxes (swap with empty HashMap)
2. Iterate and destroy each
3. Log errors but continue (best-effort cleanup)

---

## Error Handling

| Condition | Error |
|-----------|-------|
| Max sandbox limit reached | `CoreError::Connection("max sandbox limit reached (N)")` |
| Sandbox not found | `CoreError::NotFound(SandboxId)` |
| VM creation failed | Propagated from `Sandbox::create()` |
| Agent connection failed | Propagated from `Sandbox::create()` |

> [!NOTE]
> The `Connection` error variant is used for limit checks because it communicates a resource constraint, even though it's semantically about capacity rather than connectivity.

---

## Usage Examples

### Basic Workflow

```rust
// Create manager
let config = ManagerConfig::new(
    "/var/lib/bouvet/vmlinux",
    "/var/lib/bouvet/debian-devbox.ext4",
    "/usr/local/bin/firecracker",
    "/tmp/bouvet",
);
let manager = SandboxManager::new(config);

// Create a sandbox with defaults
let id = manager.create_default().await?;

// Execute a command
let result = manager.execute(id, "echo hello").await?;
println!("Output: {}", result.stdout);

// Destroy when done
manager.destroy(id).await?;
```

### With Custom Configuration

```rust
use bouvet_core::SandboxConfig;

let config = SandboxConfig::builder()
    .kernel("/custom/vmlinux")
    .rootfs("/custom/rootfs.ext4")
    .vcpus(2)
    .memory_mib(512)
    .build()?;

let id = manager.create(config).await?;
```

### Shutdown Cleanup

```rust
// On SIGINT/SIGTERM
manager.destroy_all().await?;
```

---

## Relationship to Other Components

```mermaid
graph TB
    subgraph "Layer 5: MCP"
        BouvetServer["BouvetServer"]
    end
    
    subgraph "Layer 4: Core"
        SandboxManager["SandboxManager"]
        SandboxPool["SandboxPool"]
        Sandbox["Sandbox"]
        AgentClient["AgentClient"]
    end
    
    subgraph "Layer 2: VM"
        VirtualMachine["VirtualMachine"]
    end
    
    BouvetServer -->|owns| SandboxManager
    BouvetServer -->|optionally owns| SandboxPool
    SandboxPool -->|calls register()| SandboxManager
    SandboxManager -->|manages| Sandbox
    Sandbox -->|owns| VirtualMachine
    Sandbox -->|owns| AgentClient
```

| Component | Relationship |
|-----------|--------------|
| `BouvetServer` | Owns the manager, delegates tool calls |
| `SandboxPool` | Creates sandboxes and registers them with the manager |
| `Sandbox` | Individual sandbox instances tracked by the manager |
| `VirtualMachine` | Wrapped by Sandbox, created during sandbox creation |
| `AgentClient` | Held within each Sandbox for RPC communication |

---

## Key Design Decisions

### 1. Early Lock Release

The `destroy()` method drops the write lock **before** calling `sandbox.destroy()`:

```rust
let sandbox = {
    let mut sandboxes = self.sandboxes.write().await;
    sandboxes.remove(&id)
};  // Lock released here
sandbox.destroy().await  // This is slow (VM shutdown)
```

This minimizes lock contention during the slow VM shutdown process.

### 2. No CID Reuse

CIDs are never recycled:
- **Pro**: Simpler implementation, no tracking of freed CIDs
- **Con**: Theoretical CID exhaustion after 4 billion creates
- **Practical**: A single container lifetime won't approach this limit

### 3. Error Propagation in register()

The `register()` method returns `(CoreError, Sandbox)` on failure:

```rust
Err((CoreError::Connection(...), sandbox))
```

This allows the pool to handle cleanup (destroy the rejected sandbox) rather than leaking resources.

### 4. Direct Operation Methods

Instead of forcing all operations through `with_sandbox_async()`, the manager provides direct methods like `execute()` and `read_file()`. This avoids complex lifetime issues with async closures while providing a convenient API.
