# syntax=docker/dockerfile:1
FROM rust:slim-bookworm

RUN apt-get update && apt-get install -y \
    git pkg-config libssl-dev python3 mcpp curl clang \
    && rm -rf /var/lib/apt/lists/*

# Install wasm-pack (pre-built binary — fast, cached as its own layer)
RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Install simc and ensure SimplicityHL source stays available for the WASM build.
#   1. SIMPLICITY_HL_BINARY_PATH set → pre-built Linux binary used for simc;
#      SimplicityHL is still cloned so the WASM crate can compile against it.
#   2. SIMPLICITY_HL_LOCAL_PATH set  → build simc from local source tree.
#   3. neither                       → clone from GitHub and build simc.
# In all cases /build/SimplicityHL is left in place after this step.
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
      && chmod +x /usr/local/bin/simc \
      && if [ ! -f /build/SimplicityHL/Cargo.toml ]; then \
           echo "Cloning SimplicityHL library for WASM build…" \
           && git clone --depth 1 --branch "$SIMPLICITY_HL_BRANCH" \
                "https://github.com/$SIMPLICITY_HL_REPO/SimplicityHL" /build/SimplicityHL; \
         fi; \
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
    && rm -f /build/simc_binary

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

# Build the WASM canonicalizer.
# wasm-src/ is always present in the build context (embedded in the CLI binary).
# It depends on /build/SimplicityHL which was set up in the simc step above.
COPY wasm-src /build/wasm-src
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/cargo-cache \
    echo "Building simplicityhl-wasm…" \
    && CARGO_TARGET_DIR=/cargo-cache/simplicityhl-wasm \
       wasm-pack build --target web --out-dir /wasm /build/wasm-src \
    && rm -rf /build/SimplicityHL /build/wasm-src

COPY entrypoint.sh /entrypoint.sh
COPY parse.py /parse.py
COPY transpile.py /transpile.py
RUN chmod +x /entrypoint.sh

WORKDIR /workspace
ENTRYPOINT ["/entrypoint.sh"]
