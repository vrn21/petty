//! VirtualMachine type - main interface for managing MicroVMs.

use crate::config::MachineConfig;
use crate::error::{Result, VmError};
use crate::machine_config::configure_machine;
use crate::vsock::configure_vsock;
use firepilot::builder::drive::DriveBuilder;
use firepilot::builder::executor::FirecrackerExecutorBuilder;
use firepilot::builder::kernel::KernelBuilder;
use firepilot::builder::network_interface::NetworkInterfaceBuilder;
use firepilot::builder::{Builder, Configuration};
use firepilot::machine::Machine;
use std::path::PathBuf;
use uuid::Uuid;

/// Represents a running or stopped MicroVM instance.
pub struct VirtualMachine {
    /// Unique identifier for this VM
    id: Uuid,
    /// Configuration used to create this VM
    config: MachineConfig,
    /// Current state of the VM
    state: VmState,
    /// Underlying firepilot Machine handle
    machine: Machine,
    /// Path to the Firecracker API socket
    socket_path: PathBuf,
}

/// Current state of the VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmState {
    /// VM is being created
    Creating,
    /// VM is running
    Running,
    /// VM is paused
    Paused,
    /// VM is stopped
    Stopped,
}

impl std::fmt::Display for VmState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmState::Creating => write!(f, "creating"),
            VmState::Running => write!(f, "running"),
            VmState::Paused => write!(f, "paused"),
            VmState::Stopped => write!(f, "stopped"),
        }
    }
}

impl VirtualMachine {
    /// Create and boot a new MicroVM with the given configuration.
    ///
    /// This will:
    /// 1. Build the firepilot configuration
    /// 2. Create the Machine instance
    /// 3. Configure vsock if specified
    /// 4. Start the VM
    ///
    /// # Errors
    /// Returns an error if the VM creation or startup fails.
    pub async fn create(config: MachineConfig) -> Result<Self> {
        Self::create_with_id(Uuid::new_v4(), config).await
    }

    /// Create and boot a new MicroVM with an explicit VM ID.
    ///
    /// Use this when you need to control the VM ID (e.g., to match a parent sandbox ID).
    pub async fn create_with_id(id: Uuid, config: MachineConfig) -> Result<Self> {
        let start = std::time::Instant::now();

        // Validate configuration
        tracing::trace!(%id, "Validating VM configuration");
        config.validate()?;

        tracing::info!(
            %id,
            vcpus = config.vcpu_count,
            memory_mib = config.memory_mib,
            kernel = %config.kernel_path.display(),
            rootfs = %config.root_drive.path_on_host.display(),
            "Creating new MicroVM"
        );

        // Build kernel configuration
        tracing::debug!(%id, "Building kernel configuration");
        let kernel = KernelBuilder::new()
            .with_kernel_image_path(config.kernel_path.to_string_lossy().to_string())
            .with_boot_args(config.boot_args.clone())
            .try_build()
            .map_err(|e| {
                tracing::error!(%id, error = ?e, "Failed to build kernel config");
                VmError::Config(format!("kernel config: {:?}", e))
            })?;

        // Build root drive
        tracing::debug!(%id, drive_id = %config.root_drive.drive_id, "Building root drive configuration");
        let mut drive_builder = DriveBuilder::new()
            .with_drive_id(config.root_drive.drive_id.clone())
            .with_path_on_host(config.root_drive.path_on_host.clone());

        if config.root_drive.is_root_device {
            drive_builder = drive_builder.as_root_device();
        }
        if config.root_drive.is_read_only {
            drive_builder = drive_builder.as_read_only();
        }

        let drive = drive_builder.try_build().map_err(|e| {
            tracing::error!(%id, error = ?e, "Failed to build drive config");
            VmError::Config(format!("drive config: {:?}", e))
        })?;

        // Build executor
        tracing::debug!(
            %id,
            chroot = %config.chroot_path.display(),
            firecracker = %config.firecracker_path.display(),
            "Building executor configuration"
        );
        let executor = FirecrackerExecutorBuilder::new()
            .with_chroot(config.chroot_path.to_string_lossy().to_string())
            .with_exec_binary(config.firecracker_path.clone())
            .try_build()
            .map_err(|e| {
                tracing::error!(%id, error = ?e, "Failed to build executor config");
                VmError::Config(format!("executor config: {:?}", e))
            })?;

        // Build configuration
        tracing::debug!(%id, "Building firepilot configuration");
        let mut fp_config = Configuration::new(id.to_string())
            .with_kernel(kernel)
            .with_executor(executor)
            .with_drive(drive);

        // Add extra drives
        for extra_drive in &config.extra_drives {
            tracing::trace!(%id, drive_id = %extra_drive.drive_id, "Adding extra drive");
            let mut extra_builder = DriveBuilder::new()
                .with_drive_id(extra_drive.drive_id.clone())
                .with_path_on_host(extra_drive.path_on_host.clone());

            if extra_drive.is_root_device {
                extra_builder = extra_builder.as_root_device();
            }
            if extra_drive.is_read_only {
                extra_builder = extra_builder.as_read_only();
            }

            let extra = extra_builder.try_build().map_err(|e| {
                tracing::error!(%id, error = ?e, drive_id = %extra_drive.drive_id, "Failed to build extra drive config");
                VmError::Config(format!("extra drive config: {:?}", e))
            })?;

            fp_config = fp_config.with_drive(extra);
        }

        // Add network interface if configured
        if let Some(net) = &config.network {
            tracing::debug!(%id, iface_id = %net.iface_id, host_dev = %net.host_dev_name, "Adding network interface");
            let mut net_builder = NetworkInterfaceBuilder::new()
                .with_iface_id(net.iface_id.clone())
                .with_host_dev_name(net.host_dev_name.clone());

            if let Some(mac) = &net.guest_mac {
                net_builder = net_builder.with_guest_mac(mac.clone());
            }

            let iface = net_builder.try_build().map_err(|e| {
                tracing::error!(%id, error = ?e, "Failed to build network config");
                VmError::Config(format!("network config: {:?}", e))
            })?;

            fp_config = fp_config.with_interface(iface);
        }

        // Create the machine (this starts the Firecracker process and socket)
        tracing::debug!(%id, "Creating Firecracker machine instance");
        let mut machine = Machine::new();

        machine.create(fp_config).await.map_err(|e| {
            tracing::error!(%id, error = ?e, "Failed to create machine");
            VmError::Create(format!("{:?}", e))
        })?;

        // Compute socket path: chroot_path / vm_id / firecracker.socket
        let socket_path = config
            .chroot_path
            .join(id.to_string())
            .join("firecracker.socket");
        tracing::trace!(%id, socket = %socket_path.display(), "Firecracker socket path");

        // Configure machine resources BEFORE starting the VM
        // This is required - Firecracker needs explicit vcpu/memory config
        tracing::debug!(%id, "Configuring machine resources");
        configure_machine(&socket_path, config.vcpu_count, config.memory_mib).await?;

        // Configure vsock BEFORE starting the VM (Firecracker requires this)
        if let Some(vsock_config) = &config.vsock {
            tracing::debug!(%id, cid = vsock_config.guest_cid, "Configuring vsock");
            configure_vsock(&socket_path, vsock_config).await?;
        }

        // Start the VM
        tracing::debug!(%id, "Starting VM");
        machine.start().await.map_err(|e| {
            tracing::error!(%id, error = ?e, "Failed to start VM");
            VmError::Start(format!("{:?}", e))
        })?;

        let elapsed_ms = start.elapsed().as_millis() as u64;
        tracing::info!(%id, elapsed_ms, "MicroVM started successfully");

        Ok(Self {
            id,
            config,
            state: VmState::Running,
            machine,
            socket_path,
        })
    }

    /// Get the unique ID of this VM.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Get the current state of the VM.
    pub fn state(&self) -> VmState {
        self.state
    }

    /// Get the configuration used to create this VM.
    pub fn config(&self) -> &MachineConfig {
        &self.config
    }

    /// Get the path to the Firecracker API socket.
    ///
    /// This can be used for advanced operations like configuring
    /// additional devices after VM creation.
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Get the vsock UDS path if vsock is configured.
    ///
    /// This path is used by the host to communicate with the guest agent.
    pub fn vsock_uds_path(&self) -> Option<&PathBuf> {
        self.config.vsock.as_ref().map(|v| &v.uds_path)
    }

    /// Get the vsock guest CID if vsock is configured.
    pub fn vsock_cid(&self) -> Option<u32> {
        self.config.vsock.as_ref().map(|v| v.guest_cid)
    }

    /// Start the VM (if stopped or paused).
    ///
    /// # Errors
    /// Returns an error if the VM is not in a stopped or paused state.
    pub async fn start(&mut self) -> Result<()> {
        match self.state {
            VmState::Running => {
                tracing::warn!(id = %self.id, "Cannot start: VM already running");
                return Err(VmError::InvalidState {
                    expected: "stopped or paused".into(),
                    actual: "running".into(),
                });
            }
            VmState::Creating => {
                tracing::warn!(id = %self.id, "Cannot start: VM still creating");
                return Err(VmError::InvalidState {
                    expected: "stopped or paused".into(),
                    actual: "creating".into(),
                });
            }
            _ => {}
        }

        tracing::info!(id = %self.id, state = %self.state, "Starting VM");

        self.machine.start().await.map_err(|e| {
            tracing::error!(id = %self.id, error = ?e, "Failed to start VM");
            VmError::Start(format!("{:?}", e))
        })?;

        self.state = VmState::Running;
        tracing::debug!(id = %self.id, "VM started");
        Ok(())
    }

    /// Stop the VM gracefully.
    ///
    /// # Errors
    /// Returns an error if the VM is not running.
    pub async fn stop(&mut self) -> Result<()> {
        if self.state != VmState::Running {
            tracing::warn!(id = %self.id, state = %self.state, "Cannot stop: VM not running");
            return Err(VmError::InvalidState {
                expected: "running".into(),
                actual: format!("{:?}", self.state),
            });
        }

        tracing::info!(id = %self.id, "Stopping VM gracefully");

        self.machine.stop().await.map_err(|e| {
            tracing::error!(id = %self.id, error = ?e, "Failed to stop VM");
            VmError::Stop(format!("{:?}", e))
        })?;

        self.state = VmState::Stopped;
        tracing::debug!(id = %self.id, "VM stopped");
        Ok(())
    }

    /// Force kill the VM.
    ///
    /// This immediately terminates the VM without graceful shutdown.
    pub async fn kill(&mut self) -> Result<()> {
        tracing::warn!(id = %self.id, state = %self.state, "Force killing VM");

        self.machine.kill().await.map_err(|e| {
            tracing::error!(id = %self.id, error = ?e, "Failed to kill VM");
            VmError::Stop(format!("kill failed: {:?}", e))
        })?;

        self.state = VmState::Stopped;
        tracing::debug!(id = %self.id, "VM killed");
        Ok(())
    }

    /// Destroy the VM and cleanup resources.
    ///
    /// This consumes the VirtualMachine, stopping it if running and cleaning up.
    pub async fn destroy(mut self) -> Result<()> {
        let start = std::time::Instant::now();
        tracing::info!(id = %self.id, state = %self.state, "Destroying VM");

        // Stop if running
        if self.state == VmState::Running {
            tracing::debug!(id = %self.id, "VM running, killing before destroy");
            let _ = self.kill().await;
        }

        // Machine is dropped here, which cleans up resources
        tracing::trace!(id = %self.id, "Dropping machine handle");
        drop(self.machine);

        let elapsed_ms = start.elapsed().as_millis() as u64;
        tracing::info!(id = %self.id, elapsed_ms, "VM destroyed");
        Ok(())
    }
}
