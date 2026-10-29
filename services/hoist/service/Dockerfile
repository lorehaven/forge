# syntax=docker/dockerfile:1.5
#
# Hoist's catalogue service. Built from the hoist project directory (its own cargo workspace, not forge's), so
# the context is `services/hoist`:
#
#   docker build -f service/Dockerfile -t <registry>/forge/hoist/hoist:<version> .
#
# `foreman run build-hoist-service` (and `publish-hoist-service`) do exactly that. Same musl/alpine shape as
# forge's docker/Dockerfile.alpine; the service has no config files or translations to carry along.

# ---------- Build stage ----------
FROM rust:1.98-alpine3.22 AS builder

RUN apk add --no-cache musl-dev pkgconfig build-base ca-certificates git curl openssl
RUN rustup target add x86_64-unknown-linux-musl

ARG CARGO_REGISTRIES_ENNOR_INDEX=sparse+https://ennor.ddns.net/warehouse/index/
ARG CARGO_REGISTRIES_ENNOR_TOKEN=
RUN mkdir -p /usr/local/cargo && \
    printf "[registries.ennor]\nindex = \"%s\"\n" "${CARGO_REGISTRIES_ENNOR_INDEX}" > /usr/local/cargo/config.toml && \
    if [ -n "${CARGO_REGISTRIES_ENNOR_TOKEN}" ]; then \
      printf "token = \"%s\"\n" "${CARGO_REGISTRIES_ENNOR_TOKEN}" >> /usr/local/cargo/config.toml; \
    fi

WORKDIR /usr/src/app

# Cargo reads every member's manifest (and the tests and binaries they name) to resolve the workspace, so the
# whole project is copied, minus what .dockerignore leaves out (the android project and the build output).
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release --target x86_64-unknown-linux-musl -p hoist-service

# ---------- Runtime stage ----------
FROM alpine:3.22

# CA certificates: the service fetches the three stores over https.
RUN apk add --no-cache ca-certificates

WORKDIR /app
COPY --from=builder /usr/src/app/target/x86_64-unknown-linux-musl/release/hoist-service ./service

# Root, like every other service of this estate (the deployment's securityContext decides at run time).
EXPOSE 80
ENTRYPOINT ["./service"]
