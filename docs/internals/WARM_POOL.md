# Warm Pool Architecture

> **Layer**: 4 (bouvet-core)  
> **Source**: [`crates/bouvet-core/src/pool.rs`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-core/src/pool.rs)

---

## 8.1 Purpose

The warm pool addresses the cold-start latency problem in sandbox execution:

| Metric | Cold Start | Warm Pool |
|--------|------------|-----------|
| Latency | ~500ms (VM boot + agent ready) | ~150ms (pre-booted) |
| User Experience | Noticeable delay | Near-instant |
| Resource Usage | On-demand allocation | Pre-allocated buffer |

The pool maintains a queue of pre-booted, ready-to-use sandboxes that can be instantly allocated when a request arrives. A background filler task continuously replenishes the pool to maintain the configured minimum size.

---

## 8.2 Architecture

```
┌─────────────────────────────────────────────────────┐
│                   SandboxPool                        │
├─────────────────────────────────────────────────────┤
│  pool: Arc<Mutex<VecDeque<Sandbox>>>                │
│  config: PoolConfig                                  │
│  stats: Arc<PoolStats>                               │
│  shutdown: Arc<AtomicBool>                          │
│  shutdown_notify: Arc<Notify>                        │
│  boot_semaphore: Arc<Semaphore>                      │
│  filler_handle: Option<JoinHandle<()>>              │
│  cid_counter: Arc<AtomicU32>                        │
└─────────────────────────────────────────────────────┘
         │
         │ spawns
         ▼
┌─────────────────────────────────────────────────────┐
│              Background Filler Task                  │
│  • Monitors pool level at fill_interval              │
│  • Creates sandboxes when below min_size             │
│  • Respects max_concurrent_boots via semaphore       │
│  • Responds to shutdown signal                       │
└─────────────────────────────────────────────────────┘
```

### Core Components

| Component | Type | Purpose |
|-----------|------|---------|
| `pool` | `Arc<Mutex<VecDeque<Sandbox>>>` | Thread-safe queue of warm sandboxes |
| `config` | `PoolConfig` | Pool configuration parameters |
| `stats` | `Arc<PoolStats>` | Atomic observability counters |
| `shutdown` | `Arc<AtomicBool>` | Shutdown flag for graceful termination |
| `shutdown_notify` | `Arc<Notify>` | Signal to wake filler on shutdown |
| `boot_semaphore` | `Arc<Semaphore>` | Limits concurrent VM boot operations |
| `filler_handle` | `Option<JoinHandle<()>>` | Handle to background filler task |
| `cid_counter` | `Arc<AtomicU32>` | Unique CID generator (starts at 10000) |

---

## 8.3 Configuration (PoolConfig)

```rust
pub struct PoolConfig {
    pub min_size: usize,
    pub max_concurrent_boots: usize,
    pub fill_interval: Duration,
    pub sandbox_config: SandboxConfig,
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `min_size` | `usize` | 3 | Target number of warm sandboxes to maintain |
| `max_concurrent_boots` | `usize` | 2 | Maximum parallel VM boot operations |
| `fill_interval` | `Duration` | 1 second | Interval between pool level checks |
| `sandbox_config` | `SandboxConfig` | — | VM configuration template for new sandboxes |

### Configuration Example

```rust
use bouvet_core::{SandboxPool, PoolConfig, SandboxConfig};

let config = PoolConfig {
    min_size: 5,
    max_concurrent_boots: 3,
    fill_interval: Duration::from_millis(500),
    sandbox_config: SandboxConfig::builder()
        .kernel("/var/lib/bouvet/vmlinux")
        .rootfs("/var/lib/bouvet/rootfs.ext4")
        .build()?,
    ..Default::default()
};

let mut pool = SandboxPool::new(config);
pool.start(); // Start background filler
```

---

## 8.4 Background Filler Task

The filler task is the heart of the warm pool, ensuring sandboxes are always available:

```
┌─────────────────────────────────────────────────────────┐
│                    Filler Loop                           │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  loop {                                                  │
│      tokio::select! {                                    │
│          biased;                                         │
│                                                          │
│          // Priority: shutdown signal                    │
│          _ = shutdown_notify.notified() => break;        │
│                                                          │
│          // Normal: wait for fill_interval               │
│          _ = sleep(fill_interval) => {                   │
│              if shutdown { break; }                      │
│                                                          │
│              current_size = pool.lock().len();           │
│              if current_size >= min_size { continue; }   │
│                                                          │
│              needed = min_size - current_size;           │
│              for _ in 0..needed {                        │
│                  permit = semaphore.try_acquire();       │
│                  if permit.is_err() { continue; }        │
│                                                          │
│                  spawn(create_sandbox_and_add_to_pool);  │
│              }                                           │
│          }                                               │
│      }                                                   │
│  }                                                       │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

### Key Behaviors

1. **Biased Select**: Shutdown signals take priority over fill operations
2. **Semaphore Throttling**: Prevents resource spikes by limiting concurrent boots
3. **Overfill Prevention**: Checks pool size before adding newly created sandboxes
4. **Shutdown Awareness**: Destroys sandboxes created during shutdown race conditions
5. **Non-blocking Permit Acquisition**: Uses `try_acquire_owned()` to avoid blocking

### CID Assignment

Each sandbox receives a unique vsock CID (Context ID) to prevent collisions:

```rust
cfg.vsock_cid = cid_counter.fetch_add(1, Ordering::Relaxed);
```

The pool's CID counter starts at **10000** to avoid collision with the manager's CIDs (which start at 3).

---

## 8.5 Acquisition Flow

```rust
pub async fn acquire(&self) -> Result<Sandbox, CoreError>
```

```
┌─────────────────────────────────────────────────────┐
│                  acquire() Flow                      │
├─────────────────────────────────────────────────────┤
│                                                      │
│  1. Lock pool                                        │
│     │                                                │
│     ▼                                                │
│  2. Pop front (oldest sandbox)                       │
│     │                                                │
│     ├── Some(sandbox) ──┐                            │
│     │                   ▼                            │
│     │              3. Health check (ping)            │
│     │                   │                            │
│     │     ┌─────────────┴─────────────┐              │
│     │     │                           │              │
│     │     ▼                           ▼              │
│     │  Healthy                    Unhealthy          │
│     │     │                           │              │
│     │     ▼                           ▼              │
│     │  4. Increment warm_hits     5. Destroy sandbox │
│     │     │                           │              │
│     │     ▼                           │              │
│     │  Return sandbox                 │              │
│     │                                 │              │
│     │  ◀──────────────────────────────┘              │
│     │  (retry loop)                                  │
│     │                                                │
│     └── None (pool empty)                            │
│             │                                        │
│             ▼                                        │
│         6. Increment cold_misses                     │
│             │                                        │
│             ▼                                        │
│         7. Cold-start: Sandbox::create()             │
│             │                                        │
│             ▼                                        │
│         Return new sandbox                           │
│                                                      │
└─────────────────────────────────────────────────────┘
```

### Acquisition Logic

1. **Lock Pool**: Acquire mutex to access the deque
2. **Pop Front**: FIFO ordering ensures oldest sandboxes are used first
3. **Health Check**: Call `is_healthy()` to verify agent responsiveness
4. **Warm Hit**: If healthy, increment `warm_hits` and return
5. **Discard Unhealthy**: If unhealthy, destroy and retry with next sandbox
6. **Cold Miss**: If pool exhausted, increment `cold_misses`
7. **Cold Start**: Create new sandbox on-demand (fallback path)

---

## 8.6 Statistics (PoolStats)

```rust
pub struct PoolStats {
    pub warm_hits: AtomicU64,
    pub cold_misses: AtomicU64,
    pub created: AtomicU64,
    pub destroyed: AtomicU64,
}
```

| Metric | Type | Description |
|--------|------|-------------|
| `warm_hits` | `AtomicU64` | Sandboxes served instantly from pool |
| `cold_misses` | `AtomicU64` | Requests requiring cold-start fallback |
| `created` | `AtomicU64` | Total sandboxes created by the pool |
| `destroyed` | `AtomicU64` | Total sandboxes destroyed by the pool |

### Hit Rate Calculation

```rust
pub fn hit_rate(&self) -> f64 {
    let hits = self.warm_hits() as f64;
    let misses = self.cold_misses() as f64;
    let total = hits + misses;
    if total == 0.0 {
        0.0
    } else {
        (hits / total) * 100.0
    }
}
```

### Observability Example

```rust
let stats = pool.stats();
println!("Warm hits: {}", stats.warm_hits());
println!("Cold misses: {}", stats.cold_misses());
println!("Hit rate: {:.1}%", stats.hit_rate());
println!("Created: {}", stats.created());
println!("Destroyed: {}", stats.destroyed());
```

---

## 8.7 Shutdown Sequence

```rust
pub async fn shutdown(&mut self) -> Result<(), CoreError>
```

```
┌─────────────────────────────────────────────────────┐
│                 Shutdown Sequence                    │
├─────────────────────────────────────────────────────┤
│                                                      │
│  1. Set shutdown flag                                │
│     shutdown.store(true, Ordering::Relaxed);         │
│                                                      │
│  2. Notify filler task                               │
│     shutdown_notify.notify_one();                    │
│                                                      │
│  3. Wait for filler handle                           │
│     if let Some(handle) = filler_handle.take() {     │
│         handle.await?;                               │
│     }                                                │
│                                                      │
│  4. Drain pool                                       │
│     let sandboxes = pool.lock().take();              │
│                                                      │
│  5. Destroy each sandbox                             │
│     for sandbox in sandboxes {                       │
│         sandbox.destroy().await?;                    │
│     }                                                │
│                                                      │
│  6. Log final statistics                             │
│     info!(destroyed, warm_hits, cold_misses,         │
│           hit_rate, "Pool shutdown complete");       │
│                                                      │
└─────────────────────────────────────────────────────┘
```

### Graceful Shutdown Guarantees

1. **Flag-First**: Shutdown flag is set before notification to prevent races
2. **Filler Termination**: Waits for filler task to complete before draining
3. **In-Flight Sandboxes**: Filler checks shutdown flag before/after creation
4. **Resource Cleanup**: All pooled sandboxes are destroyed one-by-one
5. **Final Statistics**: Logs hit rate and counters for observability

---

## 8.8 Thread Safety Model

| Resource | Lock Type | Contention |
|----------|-----------|------------|
| Pool queue | `Tokio Mutex` | Per-acquire, per-fill |
| Statistics | `AtomicU64` | Lock-free |
| Shutdown flag | `AtomicBool` | Lock-free |
| CID counter | `AtomicU32` | Lock-free |
| Boot permits | `Semaphore` | Non-blocking tries |

### Design Rationale

- **Tokio Mutex for Pool**: Necessary for async `await` within critical section
- **Atomics for Stats/Flags**: Lock-free reads allow concurrent observability
- **Semaphore for Boots**: Controls resource usage without blocking filler loop

---

## 8.9 API Reference

### SandboxPool Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `new` | `fn new(config: PoolConfig) -> Self` | Create pool (filler not started) |
| `start` | `fn start(&mut self)` | Start background filler task |
| `acquire` | `async fn acquire(&self) -> Result<Sandbox, CoreError>` | Get a sandbox (warm or cold) |
| `size` | `async fn size(&self) -> usize` | Current number of pooled sandboxes |
| `config` | `fn config(&self) -> &PoolConfig` | Get pool configuration |
| `stats` | `fn stats(&self) -> &PoolStats` | Get statistics reference |
| `is_running` | `fn is_running(&self) -> bool` | Check if filler is active |
| `shutdown` | `async fn shutdown(&mut self) -> Result<(), CoreError>` | Graceful shutdown |

---

## 8.10 Usage Example

```rust
use bouvet_core::{SandboxPool, PoolConfig, SandboxConfig};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure the pool
    let config = PoolConfig {
        min_size: 3,
        max_concurrent_boots: 2,
        fill_interval: Duration::from_secs(1),
        sandbox_config: SandboxConfig::builder()
            .kernel("/var/lib/bouvet/vmlinux")
            .rootfs("/var/lib/bouvet/debian-devbox.ext4")
            .build()?,
    };

    // Create and start the pool
    let mut pool = SandboxPool::new(config);
    pool.start();

    // Wait for pool to warm up
    tokio::time::sleep(Duration::from_secs(5)).await;
    println!("Pool size: {}", pool.size().await);

    // Acquire sandbox instantly (from warm pool)
    let sandbox = pool.acquire().await?;
    println!("Acquired sandbox: {}", sandbox.id());

    // Use the sandbox...
    let result = sandbox.execute("python3", "print('Hello!')").await?;
    println!("Output: {}", result.stdout);

    // Cleanup
    sandbox.destroy().await?;

    // Shutdown the pool
    pool.shutdown().await?;

    // Print final stats
    let stats = pool.stats();
    println!("Final hit rate: {:.1}%", stats.hit_rate());

    Ok(())
}
```

---

## 8.11 Performance Characteristics

| Operation | Complexity | Expected Latency |
|-----------|------------|------------------|
| `acquire()` (warm hit) | O(1) | ~150ms |
| `acquire()` (cold miss) | O(1) | ~500ms |
| `size()` | O(1) | <1ms |
| `shutdown()` | O(n) | n × ~100ms |

### Memory Overhead

Each pooled sandbox consumes:
- ~50MB VM memory (configurable)
- Firecracker process overhead
- vsock socket resources

For a pool of 3 sandboxes: ~150MB baseline memory usage.

---

## 8.12 Integration with MCP Server

The `bouvet-mcp` server integrates the pool for `create_sandbox` requests:

```rust
// In BouvetServer
async fn handle_create_sandbox(&self) -> Result<SandboxId, CoreError> {
    // Check if pool is available
    if let Some(pool) = self.pool.read().await.as_ref() {
        // Acquire from warm pool (or cold-start fallback)
        let sandbox = pool.acquire().await?;
        let id = sandbox.id();
        
        // Register in manager for tracking
        self.manager.register(sandbox);
        
        return Ok(id);
    }
    
    // Pool disabled, create directly via manager
    self.manager.create().await
}
```

The pool is optional and controlled via environment variables:
- `BOUVET_POOL_ENABLED`: Enable/disable pool (default: `true`)
- `BOUVET_POOL_MIN_SIZE`: Target pool size (default: `3`)
