//! Common types and utilities shared across the Petty sandbox platform.
//!
//! This crate provides:
//! - Core domain types (SandboxId, VMId, etc.)
//! - Error handling types
//! - Configuration structures
//! - Shared utilities

pub mod config;
pub mod error;
pub mod types;

// Re-export commonly used items
pub use error::{Error, Result};
pub use types::{SandboxId, VMId};
