//! Warm sandbox pool for reducing cold-start latency.
//!
//! This module provides a pool of pre-booted sandboxes that can be instantly
//! allocated, reducing the typical 2-5 second cold-start to sub-200ms.
//!
//! # Architecture
//!
//! The pool maintains a queue of ready-to-use sandboxes and a background
//! filler task that keeps the pool topped up to a minimum size.
//!
//! # Example
//!
//! ```ignore
//! use bouvet_core::{SandboxPool, PoolConfig, SandboxConfig};
//!
//! let config = PoolConfig {
//!     min_size: 3,
//!     sandbox_config: SandboxConfig::builder()
//!         .kernel("/path/to/vmlinux")
//!         .rootfs("/path/to/rootfs.ext4")
//!         .build()?,
//!     ..Default::default()
//! };
//!
//! let mut pool = SandboxPool::new(config);
//! pool.start(); // Start background filler
//!
//! // Acquire sandbox instantly (if pool is warm)
//! let sandbox = pool.acquire().await?;
//!
//! // Shutdown gracefully
//! pool.shutdown().await?;
//! ```

use crate::config::SandboxConfig;
use crate::error::CoreError;
use crate::sandbox::Sandbox;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Notify, Semaphore};
use tokio::task::JoinHandle;

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the sandbox pool.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Minimum number of warm sandboxes to maintain.
    ///
    /// The pool will attempt to keep at least this many sandboxes ready.
    /// Default: 3
    pub min_size: usize,

    /// Maximum number of concurrent VM boots during pool filling.
    ///
    /// This prevents resource spikes when the pool needs replenishment.
    /// Default: 2
    pub max_concurrent_boots: usize,

    /// Interval between pool fill attempts.
    ///
    /// The filler task checks pool level at this interval.
    /// Default: 1 second
    pub fill_interval: Duration,

    /// Sandbox configuration template for creating new VMs.
    pub sandbox_config: SandboxConfig,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_size: 3,
            max_concurrent_boots: 2,
            fill_interval: Duration::from_secs(1),
            sandbox_config: SandboxConfig::default(),
        }
    }
}

// ============================================================================
// Statistics
// ============================================================================

/// Pool statistics for observability.
///
/// All counters are atomic and can be read without locking.
#[derive(Debug, Default)]
pub struct PoolStats {
    /// Number of sandboxes acquired instantly from the warm pool.
    pub warm_hits: AtomicU64,
    /// Number of sandboxes that required cold-start (pool was empty).
    pub cold_misses: AtomicU64,
    /// Total sandboxes created by the pool.
    pub created: AtomicU64,
    /// Total sandboxes destroyed by the pool.
    pub destroyed: AtomicU64,
}

impl PoolStats {
    /// Get the number of warm hits.
    pub fn warm_hits(&self) -> u64 {
        self.warm_hits.load(Ordering::Relaxed)
    }

    /// Get the number of cold misses.
    pub fn cold_misses(&self) -> u64 {
        self.cold_misses.load(Ordering::Relaxed)
    }

    /// Get the total sandboxes created.
    pub fn created(&self) -> u64 {
        self.created.load(Ordering::Relaxed)
    }

    /// Get the total sandboxes destroyed.
    pub fn destroyed(&self) -> u64 {
        self.destroyed.load(Ordering::Relaxed)
    }

    /// Calculate the warm hit rate as a percentage.
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
}

// ============================================================================
// Pool Implementation
// ============================================================================

/// A pool of pre-booted sandboxes for instant allocation.
///
/// The pool maintains a queue of ready-to-use sandboxes and spawns a
/// background task to keep the pool filled to the configured minimum.
///
/// # Thread Safety
///
/// `SandboxPool` is not `Clone` or `Sync` by design. Wrap in `Arc<Mutex<_>>`
/// if shared access is required.
pub struct SandboxPool {
    /// Queue of warm, ready-to-use sandboxes.
    pool: Arc<Mutex<VecDeque<Sandbox>>>,
    /// Pool configuration.
    config: PoolConfig,
    /// Shutdown signal for the filler task.
    shutdown: Arc<AtomicBool>,
    /// Notification to wake up filler on shutdown.
    shutdown_notify: Arc<Notify>,
    /// Semaphore to limit concurrent VM boots.
    boot_semaphore: Arc<Semaphore>,
    /// Handle to the background filler task.
    filler_handle: Option<JoinHandle<()>>,
    /// Pool statistics.
    stats: Arc<PoolStats>,
    /// Counter for assigning unique vsock CIDs (starts at 3, the minimum valid CID).
    cid_counter: Arc<AtomicU32>,
}

impl SandboxPool {
    /// Create a new sandbox pool.
    ///
    /// The pool is created but the background filler is not started.
    /// Call [`start()`](Self::start) to begin filling the pool.
    pub fn new(config: PoolConfig) -> Self {
        tracing::info!(
            min_size = config.min_size,
            max_concurrent_boots = config.max_concurrent_boots,
            "Creating sandbox pool"
        );

        Self {
            pool: Arc::new(Mutex::new(VecDeque::with_capacity(config.min_size))),
            boot_semaphore: Arc::new(Semaphore::new(config.max_concurrent_boots)),
            shutdown: Arc::new(AtomicBool::new(false)),
            shutdown_notify: Arc::new(Notify::new()),
            filler_handle: None,
            stats: Arc::new(PoolStats::default()),
            cid_counter: Arc::new(AtomicU32::new(10000)), // Start at offset to avoid collision with manager's CIDs
            config,
        }
    }

    /// Start the background filler task.
    ///
    /// This spawns a tokio task that monitors the pool level and creates
    /// new sandboxes as needed to maintain `min_size`.
    pub fn start(&mut self) {
        if self.filler_handle.is_some() {
            tracing::warn!("Pool filler already started");
            return;
        }

        let pool = Arc::clone(&self.pool);
        let config = self.config.clone();
        let shutdown = Arc::clone(&self.shutdown);
        let shutdown_notify = Arc::clone(&self.shutdown_notify);
        let semaphore = Arc::clone(&self.boot_semaphore);
        let stats = Arc::clone(&self.stats);
        let cid_counter = Arc::clone(&self.cid_counter);

        let handle = tokio::spawn(async move {
            Self::filler_loop(
                pool,
                config,
                shutdown,
                shutdown_notify,
                semaphore,
                stats,
                cid_counter,
            )
            .await;
        });

        self.filler_handle = Some(handle);
        tracing::info!(min_size = self.config.min_size, "Pool filler started");
    }

    /// Background filler loop.
    ///
    /// Runs until shutdown is signaled, periodically checking pool level
    /// and spawning VM creation tasks as needed.
    async fn filler_loop(
        pool: Arc<Mutex<VecDeque<Sandbox>>>,
        config: PoolConfig,
        shutdown: Arc<AtomicBool>,
        shutdown_notify: Arc<Notify>,
        semaphore: Arc<Semaphore>,
        stats: Arc<PoolStats>,
        cid_counter: Arc<AtomicU32>,
    ) {
        tracing::debug!("Filler loop started");

        loop {
            tokio::select! {
                biased;

                // Priority: check for shutdown signal
                _ = shutdown_notify.notified() => {
                    tracing::info!("Pool filler received shutdown signal");
                    break;
                }

                // Normal: wait for fill interval
                _ = tokio::time::sleep(config.fill_interval) => {
                    // Double-check shutdown flag
                    if shutdown.load(Ordering::Relaxed) {
                        tracing::debug!("Filler detected shutdown flag");
                        break;
                    }

                    let current_size = pool.lock().await.len();
                    if current_size >= config.min_size {
                        continue;
                    }

                    let needed = config.min_size - current_size;
                    tracing::debug!(
                        current = current_size,
                        target = config.min_size,
                        needed,
                        "Pool below target, filling"
                    );

                    // Spawn creation tasks for each needed sandbox
                    for _ in 0..needed {
                        // Try to acquire a boot permit (non-blocking)
                        let permit = match semaphore.clone().try_acquire_owned() {
                            Ok(p) => p,
                            Err(_) => {
                                // At max concurrent boots, skip this one
                                tracing::trace!("Boot semaphore full, skipping");
                                continue;
                            }
                        };

                        let pool = Arc::clone(&pool);
                        let mut cfg = config.sandbox_config.clone();
                        // Assign a unique CID to prevent vsock collisions
                        cfg.vsock_cid = cid_counter.fetch_add(1, Ordering::Relaxed);
                        let stats = Arc::clone(&stats);
                        let shutdown = Arc::clone(&shutdown);
                        let min_size = config.min_size;

                        tokio::spawn(async move {
                            // Hold permit until this task completes
                            let _permit = permit;

                            // Check if shutdown was requested before expensive operation
                            if shutdown.load(Ordering::Relaxed) {
                                tracing::trace!("Skipping sandbox creation due to shutdown");
                                return;
                            }

                            tracing::debug!("Creating sandbox for pool");
                            match Sandbox::create(cfg).await {
                                Ok(sandbox) => {
                                    // Check shutdown again and pool size before adding
                                    if shutdown.load(Ordering::Relaxed) {
                                        tracing::debug!("Shutdown during sandbox creation, destroying");
                                        let _ = sandbox.destroy().await;
                                        return;
                                    }

                                    let mut guard = pool.lock().await;
                                    // Prevent pool overfill (race condition with multiple spawn tasks)
                                    if guard.len() >= min_size {
                                        drop(guard);
                                        tracing::debug!("Pool already full, destroying excess sandbox");
                                        let _ = sandbox.destroy().await;
                                        return;
                                    }
                                    stats.created.fetch_add(1, Ordering::Relaxed);
                                    guard.push_back(sandbox);
                                    let new_size = guard.len();
                                    drop(guard);
                                    tracing::debug!(pool_size = new_size, "Added sandbox to pool");
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "Failed to create sandbox for pool");
                                }
                            }
                        });
                    }
                }
            }
        }

        tracing::debug!("Filler loop exited");
    }

    /// Acquire a sandbox from the pool.
    ///
    /// This method attempts to return a sandbox from the warm pool for
    /// instant allocation. If the pool is empty, it falls back to creating
    /// a new sandbox (cold-start).
    ///
    /// Sandboxes are health-checked before being returned. Unhealthy
    /// sandboxes are discarded automatically.
    ///
    /// # Returns
    ///
    /// A ready-to-use sandbox.
    ///
    /// # Errors
    ///
    /// Returns an error if sandbox creation fails (only possible on cold-start).
    pub async fn acquire(&self) -> Result<Sandbox, CoreError> {
        // Try to get a healthy sandbox from the pool
        loop {
            let sandbox = {
                let mut pool = self.pool.lock().await;
                pool.pop_front()
            };

            let Some(sandbox) = sandbox else {
                // Pool is empty, fall back to cold-start
                break;
            };

            // Health check: ensure sandbox is still responsive
            if sandbox.is_healthy().await {
                self.stats.warm_hits.fetch_add(1, Ordering::Relaxed);
                let pool_size = self.pool.lock().await.len();
                tracing::debug!(pool_size, "Acquired sandbox from warm pool");
                return Ok(sandbox);
            }

            // Sandbox is unhealthy, destroy it and try the next one
            let sandbox_id = sandbox.id();
            tracing::warn!(sandbox_id = %sandbox_id, "Discarding unhealthy sandbox from pool");
            self.stats.destroyed.fetch_add(1, Ordering::Relaxed);
            if let Err(e) = sandbox.destroy().await {
                tracing::error!(error = %e, "Failed to destroy unhealthy sandbox");
            }
        }

        // Pool exhausted, perform cold-start
        self.stats.cold_misses.fetch_add(1, Ordering::Relaxed);
        tracing::info!("Pool empty, performing cold-start");
        let mut cfg = self.config.sandbox_config.clone();
        // Assign a unique CID to prevent vsock collisions
        cfg.vsock_cid = self.cid_counter.fetch_add(1, Ordering::Relaxed);
        Sandbox::create(cfg).await
    }

    /// Get the current number of sandboxes in the pool.
    pub async fn size(&self) -> usize {
        self.pool.lock().await.len()
    }

    /// Get the pool configuration.
    pub fn config(&self) -> &PoolConfig {
        &self.config
    }

    /// Get the pool statistics.
    pub fn stats(&self) -> &PoolStats {
        &self.stats
    }

    /// Check if the filler task is running.
    pub fn is_running(&self) -> bool {
        self.filler_handle.is_some() && !self.shutdown.load(Ordering::Relaxed)
    }

    /// Gracefully shutdown the pool.
    ///
    /// This:
    /// 1. Signals the filler task to stop
    /// 2. Waits for the filler task to complete
    /// 3. Destroys all sandboxes remaining in the pool
    ///
    /// # Errors
    ///
    /// Returns an error if any sandbox destruction fails. Errors are logged
    /// but don't stop the shutdown process.
    pub async fn shutdown(&mut self) -> Result<(), CoreError> {
        tracing::info!("Shutting down sandbox pool");

        // Signal shutdown
        self.shutdown.store(true, Ordering::Relaxed);
        self.shutdown_notify.notify_one();

        // Wait for filler to stop
        if let Some(handle) = self.filler_handle.take() {
            tracing::debug!("Waiting for filler task to complete");
            if let Err(e) = handle.await {
                tracing::error!(error = ?e, "Filler task panicked during shutdown");
            }
        }

        // Drain and destroy all pooled sandboxes
        let sandboxes: Vec<Sandbox> = {
            let mut pool = self.pool.lock().await;
            std::mem::take(&mut *pool).into_iter().collect()
        };

        let count = sandboxes.len();
        tracing::info!(count, "Destroying pooled sandboxes");

        for sandbox in sandboxes {
            let sandbox_id = sandbox.id();
            self.stats.destroyed.fetch_add(1, Ordering::Relaxed);
            if let Err(e) = sandbox.destroy().await {
                tracing::error!(
                    sandbox_id = %sandbox_id,
                    error = %e,
                    "Failed to destroy sandbox during shutdown"
                );
            }
        }

        tracing::info!(
            destroyed = count,
            warm_hits = self.stats.warm_hits(),
            cold_misses = self.stats.cold_misses(),
            hit_rate = format!("{:.1}%", self.stats.hit_rate()),
            "Pool shutdown complete"
        );

        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config_defaults() {
        let config = PoolConfig::default();
        assert_eq!(config.min_size, 3);
        assert_eq!(config.max_concurrent_boots, 2);
        assert_eq!(config.fill_interval, Duration::from_secs(1));
    }

    #[test]
    fn test_pool_stats_default() {
        let stats = PoolStats::default();
        assert_eq!(stats.warm_hits(), 0);
        assert_eq!(stats.cold_misses(), 0);
        assert_eq!(stats.created(), 0);
        assert_eq!(stats.destroyed(), 0);
    }

    #[test]
    fn test_pool_stats_hit_rate() {
        let stats = PoolStats::default();

        // No data = 0% hit rate
        assert_eq!(stats.hit_rate(), 0.0);

        // 3 hits, 1 miss = 75% hit rate
        stats.warm_hits.store(3, Ordering::Relaxed);
        stats.cold_misses.store(1, Ordering::Relaxed);
        assert!((stats.hit_rate() - 75.0).abs() < 0.01);
    }

    #[test]
    fn test_pool_new() {
        let config = PoolConfig::default();
        let pool = SandboxPool::new(config);
        assert!(!pool.is_running());
    }

    #[tokio::test]
    async fn test_pool_size_empty() {
        let config = PoolConfig::default();
        let pool = SandboxPool::new(config);
        assert_eq!(pool.size().await, 0);
    }
}
