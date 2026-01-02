//! # petty-mcp
//!
//! MCP (Model Context Protocol) server exposing Petty sandboxes to AI agents.
//!
//! This crate provides an MCP server that allows AI agents (like Claude Desktop,
//! Cursor, etc.) to create and interact with isolated code execution sandboxes.
//!
//! ## Quick Start
//!
//! Run the server with default configuration:
//!
//! ```bash
//! cargo run -p petty-mcp
//! ```
//!
//! Configure via environment variables:
//!
//! ```bash
//! export PETTY_KERNEL=/path/to/vmlinux
//! export PETTY_ROOTFS=/path/to/rootfs.ext4
//! export PETTY_FIRECRACKER=/usr/bin/firecracker
//! export PETTY_CHROOT=/tmp/petty
//! cargo run -p petty-mcp
//! ```
//!
//! ## MCP Tools
//!
//! The server exposes the following tools:
//!
//! | Tool | Description |
//! |------|-------------|
//! | `create_sandbox` | Create new isolated sandbox |
//! | `destroy_sandbox` | Destroy sandbox and release resources |
//! | `list_sandboxes` | List all active sandboxes |
//! | `execute_code` | Execute code in language (python, node, bash) |
//! | `run_command` | Execute shell command |
//! | `read_file` | Read file from sandbox |
//! | `write_file` | Write file to sandbox |
//! | `list_directory` | List directory contents |

mod config;
mod server;
mod types;

pub use config::{ConfigError, PettyConfig, MAX_COMMAND_LENGTH, MAX_INPUT_SIZE_BYTES};
pub use server::PettyServer;
pub use types::*;
