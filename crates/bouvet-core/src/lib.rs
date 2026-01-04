//! # bouvet-core
//!
//! Sandbox orchestration layer for Bouvet agentic sandboxes.
//!
//! This crate provides a high-level API for creating and managing
//! isolated code execution environments using Firecracker microVMs.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │                    bouvet-core (host)                    │
//! ├──────────────────────────────────────────────────────────┤
//! │                                                          │
//! │  ┌─────────────────┐     ┌──────────────────────────┐   │
//! │  │ SandboxManager  │────▶│  HashMap<SandboxId,      │   │
//! │  │   - create()    │     │           Sandbox>       │   │
//! │  │   - get()       │     └──────────────────────────┘   │
//! │  │   - destroy()   │                                    │
//! │  └─────────────────┘                                    │
//! │           │                                              │
//! │           ▼                                              │
//! │  ┌─────────────────┐     ┌──────────────────────────┐   │
//! │  │    Sandbox      │────▶│   VirtualMachine         │   │
//! │  │  - execute()    │     │   (from bouvet-vm)       │   │
//! │  │  - read_file()  │     └──────────────────────────┘   │
//! │  │  - write_file() │                │ vsock             │
//! │  └─────────────────┘                ▼                   │
//! │           │              ┌──────────────────────────┐   │
//! │  ┌─────────────────┐     │  Unix Socket             │   │
//! │  │  AgentClient    │────▶│  (Firecracker vsock)     │   │
//! │  │  - call()       │     └──────────────────────────┘   │
//! │  │  - ping()       │                                    │
//! │  └─────────────────┘                                    │
//! │                                                          │
//! └──────────────────────────────────────────────────────────┘
//!                           │
//!                           ▼
//! ┌──────────────────────────────────────────────────────────┐
//! │                  bouvet-agent (guest)                    │
//! │              Listening on vsock port 52                  │
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ```ignore
//! use bouvet_core::{SandboxManager, SandboxConfig, ManagerConfig};
//!
//! # async fn example() -> bouvet_core::Result<()> {
//! // Create a sandbox manager
//! let manager = SandboxManager::new(ManagerConfig::new(
//!     "/path/to/vmlinux",
//!     "/path/to/rootfs.ext4",
//!     "/usr/bin/firecracker",
//!     "/tmp/bouvet",
//! ));
//!
//! // Create a sandbox with custom configuration
//! let config = SandboxConfig::builder()
//!     .kernel("/path/to/vmlinux")
//!     .rootfs("/path/to/rootfs.ext4")
//!     .memory_mib(512)
//!     .vcpu_count(2)
//!     .build()?;
//!
//! let id = manager.create(config).await?;
//!
//! // Execute code in the sandbox
//! manager.with_sandbox_async(id, |sandbox| async move {
//!     // Execute Python code
//!     let result = sandbox.execute_code("python", "print('Hello from sandbox!')").await?;
//!     println!("Output: {}", result.stdout);
//!
//!     // Execute shell command
//!     let result = sandbox.execute("ls -la /").await?;
//!     println!("Files: {}", result.stdout);
//!
//!     // Work with files
//!     sandbox.write_file("/tmp/test.txt", "Hello, World!").await?;
//!     let content = sandbox.read_file("/tmp/test.txt").await?;
//!     println!("File content: {}", content);
//!
//!     Ok(())
//! }).await?;
//!
//! // Cleanup
//! manager.destroy(id).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Features
//!
//! - **Sandbox Lifecycle**: Create, manage, and destroy isolated execution environments
//! - **Code Execution**: Run code in Python, Node.js, Bash, and other languages
//! - **File Operations**: Read, write, and list files in the sandbox
//! - **Concurrent Access**: Thread-safe access to multiple sandboxes
//! - **Automatic Retry**: Connection retries for VM boot time tolerance
//! - **Warm Pooling**: Pre-booted sandbox pool for sub-200ms allocation

mod client;
mod config;
mod error;
mod manager;
mod pool;
mod sandbox;

pub use client::{AgentClient, ExecResult, FileEntry};
pub use config::{SandboxConfig, SandboxConfigBuilder};
pub use error::{CoreError, Result};
pub use manager::{ManagerConfig, SandboxManager};
pub use pool::{PoolConfig, PoolStats, SandboxPool};
pub use sandbox::{Sandbox, SandboxId, SandboxState};
