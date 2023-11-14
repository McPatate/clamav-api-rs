# Rust builder
FROM lukemathwalker/cargo-chef:0.1.62-rust-1.73-bullseye AS chef
WORKDIR /usr/src/app

# Plan build
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Build application
FROM chef AS builder

ARG VERSION=2.3.3
# Install Mold
RUN wget https://github.com/rui314/mold/releases/download/v${VERSION}/mold-${VERSION}-x86_64-linux.tar.gz \
    && tar -xzf mold-${VERSION}-x86_64-linux.tar.gz \
    && mv mold-${VERSION}-x86_64-linux /opt/mold \
    && rm mold-${VERSION}-x86_64-linux.tar.gz

ENV PATH=${PATH}:/opt/mold/bin

# Build dependencies - this is the caching Docker layer
COPY --from=planner /usr/src/app/recipe.json recipe.json
COPY --from=planner /usr/src/app/clamd-client clamd-client
RUN mold -run cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . .
RUN mold -run cargo build --release

# Run application
FROM debian:bullseye-slim AS runtime

WORKDIR /usr/src/app

RUN apt-get update \
    && apt-get install -y \
    ca-certificates \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/clamav-api-rs /usr/src/app/binary

ENTRYPOINT ["/usr/src/app/binary"]
