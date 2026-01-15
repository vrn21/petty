# =============================================================================
# Dockerfile.server - Production Docker Image for bouvet-mcp
# =============================================================================
# This Dockerfile creates a production-ready image containing:
# - bouvet-mcp server binary (compiled from source)
# - Firecracker binary (downloaded from GitHub)
# - Jailer binary (downloaded from GitHub)
# - vmlinux kernel (downloaded from AWS S3)
#
# The rootfs image is NOT included - it is fetched from S3 at runtime.
#
# Usage:
#   docker build -f Dockerfile.server -t bouvet-mcp:latest .
#   docker run -d --privileged --security-opt seccomp=unconfined -p 8080:8080 \
#     -e BOUVET_ROOTFS_URL=s3://your-bucket/debian-devbox.ext4 \
#     bouvet-mcp:latest
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: Chef - Dependency caching for faster rebuilds
# -----------------------------------------------------------------------------
# NOTE: Using nightly because rmcp-macros requires edition2024
FROM rustlang/rust:nightly-bookworm AS chef

RUN cargo install cargo-chef --locked
WORKDIR /app

# -----------------------------------------------------------------------------
# Stage 2: Planner - Generate dependency recipe
# -----------------------------------------------------------------------------
FROM chef AS planner

# Copy only what's needed for dependency resolution
# NOTE: Cargo.lock is gitignored, so we don't copy it - cargo will resolve fresh
COPY Cargo.toml ./
COPY crates ./crates

# Generate the recipe file for dependencies
RUN cargo chef prepare --recipe-path recipe.json

# -----------------------------------------------------------------------------
# Stage 3: Builder - Compile dependencies and application
# -----------------------------------------------------------------------------
FROM chef AS builder

# Install build dependencies
# Fix GPG errors by allowing unauthenticated packages (builder stage only)
RUN apt-get update && apt-get install -y --no-install-recommends --allow-unauthenticated \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy the recipe and build dependencies first (cached layer)
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Now copy actual source code
COPY Cargo.toml ./
COPY crates ./crates

# Build the actual application
RUN cargo build --release -p bouvet-mcp

# Strip debug symbols for smaller binary
RUN strip /app/target/release/bouvet-mcp

# -----------------------------------------------------------------------------
# Stage 4: Fetcher - Download external binaries
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim AS fetcher

# Fix GPG errors by allowing unauthenticated packages (fetcher stage only)
RUN apt-get update && apt-get install -y --no-install-recommends --allow-unauthenticated \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /downloads

# Firecracker version
ARG FC_VERSION=1.5.0
ARG TARGETARCH

# Copy and run the download script
COPY scripts/download-binaries.sh /download-binaries.sh
RUN chmod +x /download-binaries.sh && \
    if [ "$TARGETARCH" = "arm64" ]; then \
    /download-binaries.sh aarch64 "$FC_VERSION"; \
    else \
    /download-binaries.sh x86_64 "$FC_VERSION"; \
    fi

# -----------------------------------------------------------------------------
# Stage 5: Runtime - Final minimal production image
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# Labels for container metadata
LABEL org.opencontainers.image.title="bouvet-mcp"
LABEL org.opencontainers.image.description="MCP server for isolated code execution sandboxes"
LABEL org.opencontainers.image.source="https://github.com/vrn21/bouvet"
LABEL org.opencontainers.image.licenses="Apache-2.0"

# Install runtime dependencies
# Fix GPG errors by allowing unauthenticated packages
RUN apt-get update && apt-get install -y --no-install-recommends --allow-unauthenticated \
    # SSL certificates
    ca-certificates \
    # Required for healthcheck and rootfs download
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && rm -rf /var/cache/apt/*

# Create non-root user (but we need root for /dev/kvm access)
# The container runs as root to access KVM, but the actual code runs safely

# Create directories
RUN mkdir -p /var/lib/bouvet \
    && mkdir -p /tmp/bouvet \
    && mkdir -p /var/log/bouvet

# Copy binaries from builder stages
COPY --from=builder /app/target/release/bouvet-mcp /usr/local/bin/bouvet-mcp
COPY --from=fetcher /downloads/firecracker /usr/local/bin/firecracker
COPY --from=fetcher /downloads/jailer /usr/local/bin/jailer
COPY --from=fetcher /downloads/vmlinux /var/lib/bouvet/vmlinux

# Ensure binaries are executable
RUN chmod +x /usr/local/bin/bouvet-mcp \
    /usr/local/bin/firecracker \
    /usr/local/bin/jailer

# Environment variables with defaults
ENV BOUVET_KERNEL=/var/lib/bouvet/vmlinux
ENV BOUVET_ROOTFS=/var/lib/bouvet/debian-devbox.ext4
ENV BOUVET_ROOTFS_URL=https://bouvet-artifacts.s3.us-east-1.amazonaws.com/debian-devbox.ext4
ENV BOUVET_FIRECRACKER=/usr/local/bin/firecracker
ENV BOUVET_CHROOT=/tmp/bouvet
ENV BOUVET_POOL_ENABLED=true
ENV BOUVET_POOL_MIN_SIZE=3
ENV BOUVET_TRANSPORT=both
ENV BOUVET_HTTP_HOST=0.0.0.0
ENV BOUVET_HTTP_PORT=8080
ENV RUST_LOG=info


# Expose HTTP port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Copy entrypoint script (separate file for better compatibility)
COPY scripts/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
