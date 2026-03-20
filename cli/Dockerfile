FROM ubuntu:24.04

RUN apt-get update && apt-get install -y \
    git curl unzip build-essential pkg-config \
    libssl-dev ca-certificates python3 mcpp \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
ENV PATH="/root/.cargo/bin:$PATH"

# Build simc from SimplicityHL
ARG SIMPLICITY_HL_REPO=BlockstreamResearch
ARG SIMPLICITY_HL_BRANCH=master
ENV SIMPLICITY_HL_REPO=${SIMPLICITY_HL_REPO}
ENV SIMPLICITY_HL_BRANCH=${SIMPLICITY_HL_BRANCH}
RUN git clone --depth 1 --branch ${SIMPLICITY_HL_BRANCH} https://github.com/${SIMPLICITY_HL_REPO}/SimplicityHL /build/SimplicityHL \
    && cargo build --release --manifest-path /build/SimplicityHL/Cargo.toml \
    && cp /build/SimplicityHL/target/release/simc /usr/local/bin/simc \
    && rm -rf /build/SimplicityHL

# Build hal-simplicity from stringhandler's feat/add-simplicity-graph branch (PR #44)
RUN git clone --depth 1 --branch feat/add-simplicity-graph https://github.com/stringhandler/hal-simplicity /build/hal-simplicity \
    && cargo build --release --manifest-path /build/hal-simplicity/Cargo.toml \
    && cp /build/hal-simplicity/target/release/hal-simplicity /usr/local/bin/hal-simplicity \
    && rm -rf /build/hal-simplicity

COPY entrypoint.sh /entrypoint.sh
COPY parse.py /parse.py
COPY transpile.py /transpile.py
RUN chmod +x /entrypoint.sh

WORKDIR /workspace
ENTRYPOINT ["/entrypoint.sh"]
