# syntax=docker/dockerfile:1.5
#
# The image conveyor's kubernetes executor runs `.conveyor.toml` steps in for
# this workspace. Conveyor sets `working_dir=/workspace` and mounts the actual
# checkout there itself (see executors/manifest.rs's WORKSPACE_PATH) - this
# image only has to put the toolchain on PATH, not care where it runs.
#
# Unlike docker/Dockerfile.alpine's runtime images, this one keeps the full
# Rust SDK on purpose: a CI job recompiles the workspace, it does not just run
# an already-built binary.
#
# Lives outside docker/ on purpose: the workspace's `docker/*` member glob
# (root Cargo.toml) expects every direct subdirectory of docker/ to be a
# crate with its own Cargo.toml - a docker/ci/ with none broke `cargo` for the
# whole workspace, not just this image's own build.

FROM rust:1.94-alpine3.22

RUN apk add --no-cache musl-dev pkgconfig build-base ca-certificates git curl openssl openssl-dev

RUN rustup component add clippy rustfmt

# `anvil format` runs `cargo +nightly fmt` - some workspaces' rustfmt.toml
# rely on unstable options (e.g. `group_imports`) that stable rustfmt
# silently ignores instead of applying, which would make `--check` disagree
# with how the code was actually formatted.
RUN rustup toolchain install nightly --component rustfmt --no-self-update

# Same private registry alias every workspace Cargo.toml expects - without it,
# every `quench-*`/`conveyor-pipeline` dependency fails to resolve.
ARG CARGO_REGISTRIES_ENNOR_INDEX=sparse+https://ennor.ddns.net/warehouse/index/
ARG CARGO_REGISTRIES_ENNOR_TOKEN=
RUN mkdir -p /usr/local/cargo && \
    printf "[registries.ennor]\nindex = \"%s\"\n" "${CARGO_REGISTRIES_ENNOR_INDEX}" > /usr/local/cargo/config.toml && \
    if [ -n "${CARGO_REGISTRIES_ENNOR_TOKEN}" ]; then \
      printf "token = \"%s\"\n" "${CARGO_REGISTRIES_ENNOR_TOKEN}" >> /usr/local/cargo/config.toml; \
    fi

# `anvil machete` / `anvil audit` shell out to these. `--locked` uses each
# crate's own shipped Cargo.lock rather than resolving fresh - without it,
# cargo picks up whatever kstring (a cargo-audit dependency) is newest on
# crates.io, and that has repeatedly required a newer rustc than this base
# image ships.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo install --locked cargo-machete cargo-audit

# `anvil` itself, built from this workspace rather than pinned to a published
# version - a job wants the same anvil the commit it is testing was written
# against, and that only exists as source in the checkout conveyor never
# hands this build.
WORKDIR /usr/src/app
COPY Cargo.toml Cargo.lock ./
COPY cli ./cli
COPY docker ./docker
COPY examples ./examples
COPY libs ./libs
COPY tests ./tests

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release -p anvil && \
    cp target/release/anvil /usr/local/cargo/bin/anvil && \
    rm -rf /usr/src/app
