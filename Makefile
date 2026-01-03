# Petty - Makefile for building rootfs images
# Works on macOS and Linux - all Linux operations run inside Docker

.PHONY: all rootfs agent docker-image clean help check-docker rebuild

# ============================================================================
# ARCHITECTURE CONFIGURATION
# ============================================================================
# Default to ARM64 for Apple Silicon Macs running ARM64 Linux VMs
# Override with: make rootfs ARCH=x86_64
ARCH ?= aarch64

# Map architecture to Rust target and Docker platform
ifeq ($(ARCH),aarch64)
    RUST_TARGET := aarch64-unknown-linux-musl
    DOCKER_PLATFORM := linux/arm64
else ifeq ($(ARCH),x86_64)
    RUST_TARGET := x86_64-unknown-linux-musl
    DOCKER_PLATFORM := linux/amd64
else
    $(error Unsupported ARCH=$(ARCH). Use 'aarch64' or 'x86_64')
endif

# ============================================================================
# OTHER CONFIGURATION
# ============================================================================
IMAGE_SIZE ?= 2G
IMAGE_NAME ?= petty-devbox
OUTPUT_DIR := images/output
AGENT_BINARY := $(OUTPUT_DIR)/petty-agent

# Find all agent source files for dependency tracking
AGENT_SOURCES := $(shell find crates/petty-agent/src -name '*.rs' 2>/dev/null)

# ============================================================================
# TARGETS
# ============================================================================

# Default target
all: rootfs

# Help
help:
	@echo "Petty Rootfs Build System"
	@echo ""
	@echo "Targets:"
	@echo "  make rootfs       - Build complete ext4 rootfs image (default)"
	@echo "  make agent        - Cross-compile petty-agent for Linux (via Docker)"
	@echo "  make docker-image - Build Docker image only (no ext4)"
	@echo "  make clean        - Remove generated files"
	@echo "  make rebuild      - Force clean rebuild"
	@echo ""
	@echo "Configuration:"
	@echo "  ARCH=$(ARCH)            - Target architecture (aarch64 or x86_64)"
	@echo "  IMAGE_SIZE=$(IMAGE_SIZE)        - ext4 image size"
	@echo "  IMAGE_NAME=$(IMAGE_NAME)  - Docker image name"
	@echo ""
	@echo "Examples:"
	@echo "  make rootfs                 # Build for ARM64 (default)"
	@echo "  make rootfs ARCH=x86_64     # Build for x86_64"
	@echo "  make rootfs IMAGE_SIZE=4G   # Build with larger image"

# Check Docker is available
check-docker:
	@docker version > /dev/null 2>&1 || (echo "❌ Error: Docker is not running" && exit 1)
	@echo "✓ Docker is available (building for $(ARCH))"

# Build the complete rootfs ext4 image
rootfs: $(OUTPUT_DIR)/debian-devbox.ext4
	@echo "✓ Rootfs image ready: $(OUTPUT_DIR)/debian-devbox.ext4 ($(ARCH))"
	@ls -lh $(OUTPUT_DIR)/debian-devbox.ext4

# Cross-compile petty-agent for Linux inside Docker (works on macOS!)
agent: $(AGENT_BINARY)

$(AGENT_BINARY): $(AGENT_SOURCES) crates/petty-agent/Cargo.toml Cargo.toml Cargo.lock
	@echo "==> Building petty-agent for $(RUST_TARGET) (via Docker)..."
	@mkdir -p $(OUTPUT_DIR)
	DOCKER_BUILDKIT=1 docker build \
		--platform $(DOCKER_PLATFORM) \
		--build-arg RUST_TARGET=$(RUST_TARGET) \
		-f images/Dockerfile.agent \
		--output type=local,dest=$(OUTPUT_DIR) \
		.
	@chmod +x $@
	@echo "✓ Agent built: $@"

# Build the Docker image
docker-image: check-docker $(AGENT_BINARY)
	@echo "==> Building Docker image $(IMAGE_NAME) ($(DOCKER_PLATFORM))..."
	@mkdir -p images/output
	@cp $(AGENT_BINARY) images/petty-agent
	DOCKER_BUILDKIT=1 docker build \
		--platform $(DOCKER_PLATFORM) \
		-t $(IMAGE_NAME) \
		-f images/Dockerfile.devbox images/
	@rm -f images/petty-agent
	@echo "✓ Docker image built: $(IMAGE_NAME)"

# Export Docker image to tarball
$(OUTPUT_DIR)/rootfs.tar: docker-image
	@echo "==> Exporting Docker image to tarball..."
	@mkdir -p $(OUTPUT_DIR)
	@docker rm -f petty-export-temp 2>/dev/null || true
	docker create --platform $(DOCKER_PLATFORM) --name petty-export-temp $(IMAGE_NAME)
	docker export petty-export-temp > $@
	docker rm petty-export-temp
	@echo "✓ Rootfs tarball: $@"
	@ls -lh $@

# Build ext4 image from tarball (runs inside Docker, works on macOS!)
$(OUTPUT_DIR)/debian-devbox.ext4: $(OUTPUT_DIR)/rootfs.tar
	@echo "==> Building ext4 image (size: $(IMAGE_SIZE), arch: $(ARCH))..."
	@cp $(OUTPUT_DIR)/rootfs.tar images/rootfs.tar
	DOCKER_BUILDKIT=1 docker build \
		--platform $(DOCKER_PLATFORM) \
		--build-arg IMAGE_SIZE=$(IMAGE_SIZE) \
		--build-arg IMAGE_LABEL=petty-devbox \
		-f images/Dockerfile.ext4 \
		--output type=local,dest=$(OUTPUT_DIR) \
		images/
	@rm -f images/rootfs.tar
	@echo "✓ ext4 image built: $@"

# Clean generated files
clean:
	@echo "==> Cleaning..."
	rm -rf $(OUTPUT_DIR)/*
	rm -f images/petty-agent images/rootfs.tar
	docker rmi $(IMAGE_NAME) 2>/dev/null || true
	@echo "✓ Clean complete"

# Force rebuild everything
rebuild: clean all
