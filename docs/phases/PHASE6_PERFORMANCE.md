# Phase 6: High-Performance Warm Pooling

> Reducing cold start latency through pre-booted VM management.

---

## Purpose

The "Cold Start" problem (2-5 seconds for VM boot + agent ready) is a major bottleneck for responsive AI agents.
Phase 6 focuses on **Warm VM Pooling** to reduce allocation latency to **sub-200ms** by maintaining a background buffer of already-booted and ready-to-use sandboxes.

---

## Core Concept: Warm Pool

A warm pool is a thread-safe queue of `available` sandboxes that have already finished the Linux boot process and established a vsock connection with the host.

When a client (e.g., the MCP server) requests a new sandbox:

1.  **Instantly pop** a ready sandbox from the pool.
2.  **Hand over** the `Sandbox` instance to the caller.
3.  **Replenish** the pool in the background by booting a new VM.

---

## Architecture

```text
┌─────────────────────────────────────────────────────────────┐
│                       bouvet-core / pool                     │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────┐       ┌──────────────────────────┐     │
│  │   PoolManager   │◀─────▶│    VecDeque<Sandbox>     │     │
│  │  - acquire()    │       │    (Available Pool)      │     │
│  │  - replenish()  │       └──────────────────────────┘     │
│  └─────────────────┘                     │                  │
│           │                              ▼                  │
│           │                    ┌──────────────────────┐     │
│           └───────────────────▶│   Background Filler  │     │
│                                │   (Async Task)       │     │
│                                └──────────────────────┘     │
│                                          │                  │
│                                          ▼                  │
│                                ┌──────────────────────┐     │
│                                │    Sandbox::create() │     │
│                                └──────────────────────┘     │
└─────────────────────────────────────────────────────────────┘
```

---

## Technical Components

### 1. PoolManager

A high-level orchestrator that manages multiple pools (one per rootfs type).

**Behaviors:**

- **Minimum Warm Size**: Configurable buffer size (e.g., "always keep 5 Python VMs ready").
- **Acquisition**: If the pool is empty, it falls back to synchronous creation (standard cold boot).
- **Pre-warming**: On server startup, it immediately starts filling the pools to their minimum levels.

### 2. Sandbox Lifecycle Integration

The `PoolManager` becomes the primary entry point for `bouvet-mcp`, replacing direct `SandboxManager` calls for common language types.

---

## Implementation Tasks

### Task 1: Create PoolManager Struct

- Implement a thread-safe `Pool` using `Arc<Mutex<VecDeque<Sandbox>>>`.
- Define `PoolManager` to hold a map of `RootfsType -> Pool`.

### Task 2: Implement Acquisition Logic

- `acquire(config)`: Try to pop a sandbox from the matching pool.
- If successful, return it immediately.
- If empty, trigger a background fill AND optionally return a newly created VM (cold start).

### Task 3: Background Filler Task

- Long-running `tokio::spawn` loop that monitors pool levels.
- Uses `SandboxManager` to create new VMs.
- Must handle host resource exhaustion (concurrency limits for booting).

### Task 4: Integration with bouvet-mcp

- Update `BouvetServer` to use `PoolManager`.
- Measure the difference in `create_sandbox` response time.

---

## Acceptance Criteria

- [ ] `PoolManager` successfully maintains a buffer of pre-booted VMs.
- [ ] `create_sandbox` returns an ID in < 200ms when the pool is warm.
- [ ] The system automatically replenishes the pool after a sandbox is taken.
- [ ] Resource usage (CPU/RAM) is throttled during pool filling to avoid host spikes.
- [ ] Graceful shutdown correctly destroys all sandboxes in the pool.

---

## Next Steps

1.  **Phase 6 (Performance)**: Implement the Warm Pool.
2.  **Phase 7 (DevOps)**: Deploy to EC2 bare-metal.
