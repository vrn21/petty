//! VM manager abstraction layer for the Petty sandbox platform.
//!
//! This crate provides:
//! - `VMManager` trait for VM lifecycle operations
//! - Models for VM configuration and information
//! - Implementations for different VM backends (Flintlock, etc.)

pub mod manager;
pub mod models;

// Re-export main types
pub use manager::VMManager;
pub use models::{VMConfig, VMInfo};
