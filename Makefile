# Petty - Makefile for building rootfs images
# Works on macOS and Linux - all Linux operations run inside Docker

.PHONY: all rootfs agent docker-image clean help check-docker

# Configuration
IMAGE_SIZE ?= 2G
IMAGE_NAME ?= petty-devbox
OUTPUT_DIR := images/output
AGENT_TARGET := x86_64-unknown-linux-musl
AGENT_BINARY := target/$(AGENT_TARGET)/release/petty-agent

# Find all agent source files for dependency tracking
AGENT_SOURCES := $(shell find crates/petty-agent/src -name '*.rs' 2>/dev/null)

# Default target
all: rootfs

# Help
help:
	@echo "Petty Rootfs Build System"
	@echo ""
	@echo "Targets:"
	@echo "  make rootfs       - Build complete ext4 rootfs image (default)"
	@echo "  make agent        - Cross-compile petty-agent for Linux"
	@echo "  make docker-image - Build Docker image only (no ext4)"
	@echo "  make clean        - Remove generated files"
	@echo ""
	@echo "Configuration:"
	@echo "  IMAGE_SIZE=$(IMAGE_SIZE)  - ext4 image size"
	@echo "  IMAGE_NAME=$(IMAGE_NAME)  - Docker image name"

# Check Docker is available and BuildKit is enabled
check-docker:
	@docker version > /dev/null 2>&1 || (echo "❌ Error: Docker is not running" && exit 1)
	@echo "✓ Docker is available"

# Build the complete rootfs ext4 image
rootfs: $(OUTPUT_DIR)/debian-devbox.ext4
	@echo "✓ Rootfs image ready: $(OUTPUT_DIR)/debian-devbox.ext4"
	@ls -lh $(OUTPUT_DIR)/debian-devbox.ext4

# Cross-compile petty-agent for Linux (static musl binary)
# Rebuilds if any source file changes
agent: $(AGENT_BINARY)

$(AGENT_BINARY): $(AGENT_SOURCES) crates/petty-agent/Cargo.toml
	@echo "==> Building petty-agent for $(AGENT_TARGET)..."
	@rustup target add $(AGENT_TARGET) 2>/dev/null || true
	cargo build -p petty-agent --release --target $(AGENT_TARGET)
	@echo "✓ Agent built: $@"

# Build the Docker image
docker-image: check-docker $(AGENT_BINARY)
	@echo "==> Building Docker image $(IMAGE_NAME)..."
	@mkdir -p images/output
	@cp $(AGENT_BINARY) images/petty-agent
	DOCKER_BUILDKIT=1 docker build -t $(IMAGE_NAME) -f images/Dockerfile.devbox images/
	@rm -f images/petty-agent
	@echo "✓ Docker image built: $(IMAGE_NAME)"

# Export Docker image to tarball
$(OUTPUT_DIR)/rootfs.tar: docker-image
	@echo "==> Exporting Docker image to tarball..."
	@mkdir -p $(OUTPUT_DIR)
	@docker rm -f petty-export-temp 2>/dev/null || true
	docker create --name petty-export-temp $(IMAGE_NAME)
	docker export petty-export-temp > $@
	docker rm petty-export-temp
	@echo "✓ Rootfs tarball: $@"
	@ls -lh $@

# Build ext4 image from tarball (runs inside Docker, works on macOS!)
$(OUTPUT_DIR)/debian-devbox.ext4: $(OUTPUT_DIR)/rootfs.tar
	@echo "==> Building ext4 image (size: $(IMAGE_SIZE))..."
	@cp $(OUTPUT_DIR)/rootfs.tar images/rootfs.tar
	DOCKER_BUILDKIT=1 docker build \
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
