# Use this with
#
# docker build . -t xrc
# container_id=$(docker create xrc no-op)
# docker cp $container_id:xrc.wasm xrc.wasm
# docker rm --volumes $container_id

# This is the "builder", i.e. the base image used later to build the final
# code.
FROM ubuntu:20.04 as builder
SHELL ["bash", "-c"]

ARG rust_version=1.62.1
ENV NODE_VERSION=16.13.2

ENV TZ=UTC

RUN ln -snf /usr/share/zoneinfo/$TZ /etc/localtime && echo $TZ > /etc/timezone && \
    apt -yq update && \
    apt -yqq install --no-install-recommends curl ca-certificates \
    build-essential pkg-config libssl-dev llvm-dev liblmdb-dev clang cmake \
    git jq

# Install Rust and Cargo in /opt
ENV RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    PATH=/opt/cargo/bin:$PATH

RUN curl --fail https://sh.rustup.rs -sSf \
    | sh -s -- -y --default-toolchain ${rust_version}-x86_64-unknown-linux-gnu --no-modify-path && \
    rustup default ${rust_version}-x86_64-unknown-linux-gnu && \
    rustup target add wasm32-unknown-unknown

ENV PATH=/cargo/bin:$PATH

# Install ic-wasm and wasm-opt
RUN cargo install wasm-opt --version 0.110.2
RUN cargo install ic-wasm --version 0.3.0

# Pre-build all cargo dependencies. Because cargo doesn't have a build option
# to build only the dependencies, we pretend that our project is a simple, empty
# `lib.rs`. Then we remove the dummy source files to make sure cargo rebuild
# everything once the actual source code is COPYed (and e.g. doesn't trip on
# timestamps being older)
COPY Cargo.lock .
COPY Cargo.toml .
COPY src/xrc-tests/Cargo.toml src/xrc-tests/Cargo.toml
COPY src/xrc/Cargo.toml src/xrc/Cargo.toml
RUN mkdir -p src/xrc-tests/src && \
    touch src/xrc-tests/src/lib.rs && \
    mkdir -p src/xrc/src && \
    touch src/xrc/src/lib.rs && \
    cargo build --target wasm32-unknown-unknown --release --package xrc && \
    rm -rf src/xrc/ &&\
    rm -rf src/xrc-tests/

# Install dfx
COPY dfx.json dfx.json
RUN DFX_VERSION="$(jq -cr .dfx dfx.json)" sh -ci "$(curl -fsSL https://sdk.dfinity.org/install.sh)"

# Start the second container
FROM builder AS build
SHELL ["bash", "-c"]
ARG DFX_NETWORK=mainnet
RUN echo "DFX_NETWORK: '$DFX_NETWORK'"

ARG OWN_CANISTER_ID
RUN echo "OWN_CANISTER_ID: '$OWN_CANISTER_ID'"

# Build
# ... put only git-tracked files in the build directory
COPY . /build
WORKDIR /build
# Creates the wasm without creating the canister
RUN cargo build -p xrc --target wasm32-unknown-unknown --release
RUN ic-wasm /build/target/wasm32-unknown-unknown/release/xrc.wasm -o /build/target/wasm32-unknown-unknown/release/xrc.wasm
RUN wasm-opt -Os -o /build/target/wasm32-unknown-unknown/release/xrc.wasm /build/target/wasm32-unknown-unknown/release/xrc.wasm

RUN ls -sh /build
RUN ls -sh /build/target/wasm32-unknown-unknown/release/xrc.wasm; sha256sum /build/target/wasm32-unknown-unknown/release/xrc.wasm

FROM scratch AS scratch
COPY --from=build /build/target/wasm32-unknown-unknown/release/xrc.wasm /
