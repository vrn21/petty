# petty-core

Sandbox orchestration. Creates VMs, connects agents, manages lifecycle.

## Build

```
cargo build -p petty-core --release
```

## Usage

```rust
let manager = SandboxManager::new(ManagerConfig::new(kernel, rootfs, firecracker, chroot));
let id = manager.create(SandboxConfig::builder().kernel(k).rootfs(r).build()?).await?;
manager.execute(id, "echo hello").await?;
manager.destroy(id).await?;
```

## SandboxManager

Thread-safe. Methods: `new`, `create`, `create_default`, `register`, `with_sandbox_async`, `destroy`, `destroy_all`, `list`, `count`, `exists`, `execute`, `execute_code`, `read_file`, `write_file`, `list_dir`.

`register(sandbox)` returns `(CoreError, Sandbox)` on failure for cleanup.

## Sandbox

Methods: `id`, `state`, `execute`, `execute_code`, `read_file`, `write_file`, `list_dir`, `is_healthy`, `destroy`.

## SandboxConfig

Builder: `.kernel(path)` `.rootfs(path)` `.memory_mib(256)` `.vcpu_count(2)` `.vsock_cid(3)` `.build()?`

## SandboxPool (Warm Pool)

Pre-booted VMs for <200ms allocation.

```rust
let mut pool = SandboxPool::new(PoolConfig { min_size: 3, sandbox_config: cfg, ..Default::default() });
pool.start();
let sb = pool.acquire().await?; // instant if warm
pool.shutdown().await?;
```

Methods: `new`, `start`, `acquire`, `size`, `stats`, `is_running`, `shutdown`.

PoolConfig: `min_size(3)`, `max_concurrent_boots(2)`, `fill_interval(1s)`, `sandbox_config`.

PoolStats: `warm_hits`, `cold_misses`, `created`, `destroyed`, `hit_rate`.

## Types

SandboxId: UUID wrapper, Display/Hash/Eq.

FileEntry: `name`, `is_dir`, `size` (from list_dir).

ExecResult: `exit_code`, `stdout`, `stderr`, `success()`.

## ManagerConfig

`kernel_path`, `rootfs_path`, `firecracker_path`, `chroot_path`, `max_sandboxes(100)`.

## CoreError

`Vm`, `Connection`, `AgentTimeout`, `Rpc`, `NotFound`, `InvalidState`, `Json`, `Io`.

## Connection

Vsock via {chroot}/v.sock. Sends "CONNECT 52\n", reads "OK <port>\n", exchanges JSON-RPC. Retry: 100ms/10s.

## Files

lib.rs, config.rs, error.rs, client.rs, sandbox.rs, manager.rs, pool.rs

## Limits

max_sandboxes=100, connect_timeout=10s, rpc_timeout=30s, vsock_cid>=3, pool_min=3, pool_max_boots=2
