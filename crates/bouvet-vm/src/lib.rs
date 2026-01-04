//! # bouvet-vm
//!
//! MicroVM management layer for Bouvet agentic sandbox.
//! Provides a high-level abstraction over firepilot/Firecracker.
//!
//! ## Quick Start
//!
//! ```no_run
//! use bouvet_vm::VmBuilder;
//!
//! # async fn example() -> bouvet_vm::Result<()> {
//! // Create and start a VM using the builder pattern
//! let vm = VmBuilder::new()
//!     .vcpus(2)
//!     .memory_mib(256)
//!     .kernel("/path/to/vmlinux")
//!     .rootfs("/path/to/rootfs.ext4")
//!     .build()
//!     .await?;
//!
//! // VM is now running
//! assert_eq!(vm.state(), bouvet_vm::VmState::Running);
//!
//! // Cleanup
//! vm.destroy().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Features
//!
//! - **Lifecycle Management**: Create, start, stop, kill, and destroy MicroVMs
//! - **Drive Configuration**: Root filesystem and additional drives
//! - **Network Configuration**: TAP device support for guest networking
//! - **vsock Support**: Guest-host communication channel (when supported)
//! - **Builder Pattern**: Ergonomic configuration with `VmBuilder`

mod builder;
mod config;
mod error;
mod machine;
mod vsock;

pub use builder::VmBuilder;
pub use config::{DriveConfig, MachineConfig, NetworkConfig, VsockConfig};
pub use error::{Result, VmError};
pub use machine::{VirtualMachine, VmState};
