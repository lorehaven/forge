# syntax=docker/dockerfile:1.5
#
# The image conveyor's kubernetes executor runs `.conveyor.toml` steps in for
# this workspace. Conveyor sets `working_dir=/workspace` and mounts the actual
# checkout there itself (see executors/manifest.rs's WORKSPACE_PATH) - this
# image only has to put the toolchain on PATH, not care where it runs.
#
# Unlike images/Dockerfile.alpine's runtime images, this one keeps the full
# Rust SDK on purpose: a CI job recompiles the workspace, it does not just run
# an already-built binary.
#
# Lives in images/ with the other Dockerfiles: it is a tool image, not a service, so it has no place under
# services/ (and no crate to be a workspace member).
#
# glibc (Debian), not Alpine/musl: this image never ships anything, it only
# runs CI checks, so there's no reason to pay musl's cost here. And it's a
# real cost, not just size - cargo-semver-checks probes rustc for every
# crate-type at once (bin/rlib/dylib/cdylib/staticlib/proc-macro) to learn
# target file-naming conventions, and that combined query flat-out crashes
# rustc on a musl host (SIGABRT) instead of erroring cleanly. Retargeting the
# probe at gnu explicitly sidesteps the crash but then any crate with a
# cc-rs build script fails for real, needing a gnu cross-compiler this image
# doesn't have. glibc-native has neither problem.

FROM rust:1.98-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config ca-certificates git curl libssl-dev perl rsync \
    && rm -rf /var/lib/apt/lists/*

# The Docker CLI alone (no daemon): a release job builds and pushes the service images through the privileged
# `docker:dind` service of its pod, reached with DOCKER_HOST (see the `release-crates` stage of .conveyor.toml).
ARG DOCKER_CLI_VERSION=27.5.1
RUN curl -fsSL "https://download.docker.com/linux/static/stable/x86_64/docker-${DOCKER_CLI_VERSION}.tgz" \
    | tar -xz --strip-components=1 -C /usr/local/bin docker/docker

# The golden-file parity jobs of sci-rust (`numpy-parity`, `numpy-bench`, `python-parity`) run the generators with
# python3 and compare against files made with one NumPy version (2.5.4, which needs Python >= 3.12; Debian bookworm
# ships 3.11 and NumPy 1.24, whose results differ). `uv` brings its own Python and the pinned NumPy into /opt/numpy, and
# that venv leads PATH, so `python3` in a job is that one. Bump NUMPY_VERSION and SCIPY_VERSION together with sci-rust's
# num-rs/tests/golden/ndarray_cases.tsv (its first line names the version).
ARG UV_VERSION=0.13.7
ARG PYTHON_VERSION=3.14
ARG NUMPY_VERSION=2.5.4
ARG SCIPY_VERSION=1.18.1
RUN curl -fsSL "https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/uv-x86_64-unknown-linux-gnu.tar.gz" \
    | tar -xz --strip-components=1 -C /usr/local/bin uv-x86_64-unknown-linux-gnu/uv uv-x86_64-unknown-linux-gnu/uvx
ENV UV_PYTHON_INSTALL_DIR=/opt/uv-python
RUN uv venv --python "${PYTHON_VERSION}" /opt/numpy \
    && uv pip install --python /opt/numpy/bin/python "numpy==${NUMPY_VERSION}" "scipy==${SCIPY_VERSION}" \
    && chmod -R a+rX /opt/numpy /opt/uv-python
ENV PATH=/opt/numpy/bin:${PATH}

RUN rustup component add clippy rustfmt

# `anvil format` runs `cargo +nightly fmt` - some workspaces' rustfmt.toml
# rely on unstable options (e.g. `group_imports`) that stable rustfmt
# silently ignores instead of applying, which would make `--check` disagree
# with how the code was actually formatted.
RUN rustup toolchain install nightly --component rustfmt --no-self-update

# `anvil nextest`'s coverage counterpart isn't wired into anvil itself, but
# `cargo llvm-cov` needs this component regardless of how it's invoked.
RUN rustup component add llvm-tools-preview

# Same private registry alias every workspace Cargo.toml expects - without it,
# every `quench-*`/`conveyor-pipeline` dependency fails to resolve.
ARG CARGO_REGISTRIES_ENNOR_INDEX=sparse+https://ennor.ddns.net/warehouse/index/
ARG CARGO_REGISTRIES_ENNOR_TOKEN=
RUN mkdir -p /usr/local/cargo && \
    printf "[registries.ennor]\nindex = \"%s\"\n" "${CARGO_REGISTRIES_ENNOR_INDEX}" > /usr/local/cargo/config.toml && \
    if [ -n "${CARGO_REGISTRIES_ENNOR_TOKEN}" ]; then \
      printf "token = \"%s\"\n" "${CARGO_REGISTRIES_ENNOR_TOKEN}" >> /usr/local/cargo/config.toml; \
    fi

# `cargo fuzz` runs the nightly security job's coverage-guided targets (quench/fuzz) on the nightly
# toolchain installed above.
#
# `anvil machete` / `anvil audit` / `anvil deny` / `anvil nextest` /
# `anvil semver-check` and the plain `cargo llvm-cov` step all shell out to
# these. `--locked` uses each crate's own shipped Cargo.lock rather than
# resolving fresh - without it, cargo picks up whatever kstring (a
# cargo-audit dependency) is newest on crates.io, and that has repeatedly
# required a newer rustc than this base image ships.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo install --locked cargo-machete cargo-audit cargo-deny cargo-nextest cargo-llvm-cov cargo-semver-checks cargo-fuzz

# `anvil` itself, built from this workspace rather than pinned to a published
# version - a job wants the same anvil the commit it is testing was written
# against, and that only exists as source in the checkout conveyor never
# hands this build.
#
# `riveter` rides along for the same reason, and for a second one: a deployment
# repository's pipeline (`riveter publish`) runs in this image, and a riveter
# older than the pipeline's steps would parse there and then fail at run time.
# Pack and publish need no kubectl, so none is installed.
WORKDIR /usr/src/app
COPY Cargo.toml Cargo.lock ./
COPY cli ./cli
COPY services ./services
COPY libs ./libs
COPY tests ./tests

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release -p anvil -p riveter && \
    cp target/release/anvil target/release/riveter /usr/local/cargo/bin/ && \
    rm -rf /usr/src/app

# Once the registry requires a token to read, Cargo asks `anvil registry-token` for one whenever it needs
# it: a fresh short-lived bearer token each time, from the job's ANVIL_APP_TOKEN / ANVIL_GATEHOUSE_URL
# secrets. Added only now, after anvil exists - the build above has to resolve crates without it (with
# the CARGO_REGISTRIES_ENNOR_TOKEN build argument, if the registry already asks for one).
RUN printf 'credential-provider = ["cargo:token-from-stdout", "anvil", "registry-token"]\n' >> /usr/local/cargo/config.toml
