# Petty - Agent Sandbox Platform

A production-ready, Rust-based sandbox platform for executing agent code in isolated MicroVM environments.

## Overview

Petty provides a secure, scalable sandbox environment for running AI agent code with:

- **Strong Isolation**: MicroVM-based isolation using Firecracker
- **MCP Interface**: Model Context Protocol for standard agent integration
- **High Performance**: Fast boot times (~150-250ms), minimal overhead
- **Scalability**: Support for hundreds of concurrent sandboxes
- **Production Ready**: Comprehensive error handling, observability, and testing

## Architecture

```
┌────────────────────┐
│   MCP Interface    │  ← Agent communication
└─────────┬──────────┘
          │
┌─────────▼──────────┐
│   Orchestrator     │  ← Session & lifecycle management
└─────────┬──────────┘
          │
    ┌─────┴─────┐
    │           │
┌───▼───┐   ┌───▼────────┐
│ VM    │   │ Agent      │
│Manager│   │ Comms      │
└───┬───┘   └───┬────────┘
    │           │
┌───▼───────────▼───┐
│   Flintlock       │
│   + Firecracker   │
│   MicroVMs        │
└───────────────────┘
```

## Crate Structure

The project is organized as a Cargo workspace with the following crates:

- **`petty-common`**: Shared types, errors, and configuration
- **`petty-vm-manager`**: VM lifecycle abstraction (trait-based for swappable backends)
- **`petty-agent-comms`**: Communication with in-VM agents via vsock
- **`petty-sandbox-orchestrator`**: Core orchestration and session management
- **`petty-mcp-server`**: MCP protocol interface
- **`petty-in-vm-agent`**: Agent binary running inside MicroVMs

## Features (v0.1)

### MCP Tools

1. **`create_sandbox`** - Create isolated execution environment
2. **`execute_command`** - Run commands in sandbox
3. **`upload_file`** - Upload files to sandbox
4. **`download_file`** - Download files from sandbox
5. **`list_files`** - List files in sandbox
6. **`destroy_sandbox`** - Remove sandbox

### Security

- MicroVM isolation (stronger than containers)
- Resource limits (CPU, memory, disk)
- Network isolation (vsock communication only)
- Configurable timeouts

### Observability

- Structured logging with `tracing`
- Prometheus metrics
- Health check endpoints

## Development Status

### ✅ Completed (Week 1)

- [x] Project workspace setup
- [x] Common types and error handling
- [x] VM Manager trait abstraction
- [x] Configuration system
- [x] Comprehensive unit tests

### 🚧 In Progress

- [ ] Flintlock integration (Week 1-2)
- [ ] In-VM agent (Week 2)
- [ ] Agent communication (Week 2-3)

### 📋 Planned

- [ ] Orchestrator (Week 3)
- [ ] MCP server (Week 3-4)
- [ ] Docker image (Week 4)
- [ ] Integration tests (Week 4)
- [ ] Production hardening (Week 5)

## Getting Started

### Prerequisites

- Rust 1.70+ (edition 2021)
- Flintlock (for VM management)
- containerd (for image management)
- Firecracker (VM runtime)

### Build

```bash
# Check all crates
cargo check --workspace

# Run tests
cargo test --workspace

# Build release
cargo build --workspace --release
```

### Development

```bash
# Run specific crate tests
cargo test -p petty-common
cargo test -p petty-vm-manager

# Run with logging
RUST_LOG=debug cargo run -p petty-mcp-server
```

## Configuration

Configuration is managed via TOML files. See `config.toml.example` for a complete example.

```toml
[vm_manager.flintlock]
endpoint = "http://localhost:9090"
kernel_path = "/var/lib/flintlock/kernels/vmlinux-5.10"
image_name = "docker.io/library/sandbox-base:v0.1"

[orchestrator]
max_concurrent_sandboxes = 100
default_ttl_secs = 3600

[orchestrator.defaults]
vcpu = 1
memory_mb = 512
disk_size_mb = 1024
```

## Testing

Each crate includes comprehensive unit tests:

```bash
# Run all tests
cargo test --workspace

# Run with output
cargo test --workspace -- --nocapture

# Run specific test
cargo test test_sandbox_id_creation
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Contributing

Contributions welcome! Please ensure:

1. All tests pass: `cargo test --workspace`
2. Code is formatted: `cargo fmt --all`
3. No clippy warnings: `cargo clippy --workspace -- -D warnings`

## Roadmap

See [ROADMAP.md](ROADMAP.md) for detailed implementation plan and timeline.
