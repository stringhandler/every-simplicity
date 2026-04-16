# syntax=docker/dockerfile:1
FROM rust:slim-bookworm

RUN apt-get update && apt-get install -y \
    git pkg-config libssl-dev python3 mcpp curl \
    && rm -rf /var/lib/apt/lists/*

# Install simc — three modes, checked in order:
#   1. SIMPLICITY_HL_BINARY_PATH set → pre-built Linux binary copied in directly (fastest)
#   2. SIMPLICITY_HL_LOCAL_PATH set  → build from local source tree copied into context
#   3. neither                       → clone from GitHub and build
ARG SIMPLICITY_HL_REPO=BlockstreamResearch
ARG SIMPLICITY_HL_BRANCH=master
COPY simc_binary /build/simc_binary
COPY SimplicityHL /build/SimplicityHL
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/cargo-cache \
    if [ -s /build/simc_binary ]; then \
      echo "Installing pre-built simc binary…" \
      && cp /build/simc_binary /usr/local/bin/simc \
      && chmod +x /usr/local/bin/simc; \
    elif [ -f /build/SimplicityHL/Cargo.toml ]; then \
      echo "Building simc from local source…" \
      && CARGO_TARGET_DIR=/cargo-cache/simc \
         cargo build --release --manifest-path /build/SimplicityHL/Cargo.toml \
      && cp /cargo-cache/simc/release/simc /usr/local/bin/simc; \
    else \
      echo "Cloning simc from GitHub ($SIMPLICITY_HL_REPO @ $SIMPLICITY_HL_BRANCH)…" \
      && git clone --depth 1 --branch "$SIMPLICITY_HL_BRANCH" \
           "https://github.com/$SIMPLICITY_HL_REPO/SimplicityHL" /build/SimplicityHL \
      && CARGO_TARGET_DIR=/cargo-cache/simc \
         cargo build --release --manifest-path /build/SimplicityHL/Cargo.toml \
      && cp /cargo-cache/simc/release/simc /usr/local/bin/simc; \
    fi \
    && rm -rf /build/SimplicityHL /build/simc_binary

# Build hal-simplicity from stringhandler's feat/add-simplicity-graph branch (PR #44)
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/cargo-cache \
    git clone --depth 1 --branch feat/add-simplicity-graph \
      https://github.com/stringhandler/hal-simplicity /build/hal-simplicity \
    && CARGO_TARGET_DIR=/cargo-cache/hal-simplicity \
       cargo build --release --manifest-path /build/hal-simplicity/Cargo.toml \
    && cp /cargo-cache/hal-simplicity/release/hal-simplicity /usr/local/bin/hal-simplicity \
    && rm -rf /build/hal-simplicity

COPY entrypoint.sh /entrypoint.sh
COPY parse.py /parse.py
COPY transpile.py /transpile.py
RUN chmod +x /entrypoint.sh

WORKDIR /workspace
ENTRYPOINT ["/entrypoint.sh"]
