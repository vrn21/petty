//! # petty-mcp
//!
//! MCP (Model Context Protocol) server exposing Petty sandboxes to AI agents.
//!
//! This crate provides an MCP server that allows AI agents (like Claude Desktop,
//! Cursor, remote agents, etc.) to create and interact with isolated code
//! execution sandboxes.
//!
//! ## Quick Start
//!
//! Run the server (enables both stdio and HTTP by default):
//!
//! ```bash
//! cargo run -p petty-mcp
//! ```
//!
//! The server will listen on:
//! - **stdio** for local AI tools (Claude Desktop, Cursor)
//! - **HTTP :8080** for remote AI agents
//!
//! ## Configuration
//!
//! Configure via environment variables:
//!
//! ```bash
//! # VM resources
//! export PETTY_KERNEL=/path/to/vmlinux
//! export PETTY_ROOTFS=/path/to/rootfs.ext4
//! export PETTY_FIRECRACKER=/usr/bin/firecracker
//! export PETTY_CHROOT=/tmp/petty
//!
//! # Transport mode (default: both)
//! export PETTY_TRANSPORT=both  # stdio, http, or both
//!
//! # HTTP server
//! export PETTY_HTTP_HOST=0.0.0.0
//! export PETTY_HTTP_PORT=8080
//!
//! # Warm pool
//! export PETTY_POOL_ENABLED=true
//! export PETTY_POOL_MIN_SIZE=3
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
pub mod http;
mod server;
mod types;

pub use config::{ConfigError, PettyConfig, TransportMode, MAX_COMMAND_LENGTH, MAX_INPUT_SIZE_BYTES};
pub use http::build_router;
pub use server::PettyServer;
pub use types::*;
