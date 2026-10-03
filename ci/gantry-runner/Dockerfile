# syntax=docker/dockerfile:1.5
#
# The image a Gantry runner Job runs: `gantry-runner` (executes the plan), `riveter` (pull, install) and
# `kubectl` (scale, wait, roll back) - and nothing else. Not rust-builder, which has no kubectl and ships a
# multi-gigabyte Rust SDK for a job that only runs three programs.
#
# Built from this workspace, like rust-builder, so the riveter in the runner is always the one the commit
# was written against: a plan that uses a new riveter flag must not meet an older riveter at run time.
# Lives outside docker/ for the same reason rust-builder does (every direct child of docker/ must be a crate).
#
#   docker build -f ci/gantry-runner/Dockerfile -t ossiriand.arda:8080/forge/gantry/runner:<gantry version> .
#
# Tag it with Gantry's version: the service names the image, and the two are released together.

FROM rust:1.98-alpine3.22 AS builder

RUN apk add --no-cache musl-dev pkgconfig build-base ca-certificates git curl openssl
RUN rustup target add x86_64-unknown-linux-musl

ARG CARGO_REGISTRIES_ENNOR_INDEX=sparse+https://ennor.ddns.net/index/
ARG CARGO_REGISTRIES_ENNOR_TOKEN=
RUN mkdir -p /usr/local/cargo && \
    printf "[registries.ennor]\nindex = \"%s\"\n" "${CARGO_REGISTRIES_ENNOR_INDEX}" > /usr/local/cargo/config.toml && \
    if [ -n "${CARGO_REGISTRIES_ENNOR_TOKEN}" ]; then \
      printf "token = \"%s\"\n" "${CARGO_REGISTRIES_ENNOR_TOKEN}" >> /usr/local/cargo/config.toml; \
    fi

WORKDIR /usr/src/app
COPY Cargo.toml Cargo.lock ./
COPY cli ./cli
COPY docker ./docker
COPY examples ./examples
COPY libs ./libs
COPY tests ./tests

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release --target x86_64-unknown-linux-musl -p riveter -p gantry-service --bin riveter --bin gantry-runner

FROM alpine:3.22

RUN apk add --no-cache ca-certificates kubectl

COPY --from=builder /usr/src/app/target/x86_64-unknown-linux-musl/release/riveter /usr/local/bin/riveter
COPY --from=builder /usr/src/app/target/x86_64-unknown-linux-musl/release/gantry-runner /usr/local/bin/gantry-runner

# Not root: the Job's service account is what carries the privileges, and a shell in this container
# should not also be able to write to the image.
RUN adduser -D -u 10001 runner && mkdir -p /work && chown runner /work
USER runner
WORKDIR /work
ENTRYPOINT ["gantry-runner"]
