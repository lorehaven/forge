# syntax=docker/dockerfile:1.5
#
# The image the `release-artifacts` stage of .conveyor.toml runs in: rust-builder (git, cargo, anvil, the registry
# alias) plus what `anvil artifact release` needs to build the apps: a JDK for Gradle, the Android SDK and NDK,
# the Rust targets for them, and `quench` (it makes the APK and the .deb/.rpm/.pkg.tar.zst packages). The packages need
# no packers: quench writes the archives itself.
#
# UNVERIFIED: written to match the toolchain on the development machine (SDK platform 36, build-tools 36, NDK 28+,
# JDK 21, Gradle from the project's wrapper); it has not been built. Build it from the repository root:
#   docker build -f images/rust-android-builder/Dockerfile \
#     --build-arg CARGO_REGISTRIES_ENNOR_TOKEN=... -t ossiriand.arda:8080/forge/ci/rust-android-builder:latest .
#
# Gradle downloads its own distribution and the Android Gradle plugin on the first build in a job, so the pod needs
# network access to services.gradle.org and Google's Maven repository; nothing is baked in to avoid that.

ARG BASE=ossiriand.arda:8080/forge/ci/rust-builder:latest
FROM ${BASE}

ARG JDK_MAJOR=21
ARG ANDROID_CMDLINE_TOOLS=11076708
ARG ANDROID_PLATFORM=36
ARG ANDROID_BUILD_TOOLS=36.0.0
ARG ANDROID_NDK=28.2.13676358

# Debian bookworm ships JDK 17; the Android Gradle plugin the apps use wants 21, so it comes from Temurin.
RUN apt-get update && apt-get install -y --no-install-recommends unzip \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /opt/java \
    && curl -fsSL "https://api.adoptium.net/v3/binary/latest/${JDK_MAJOR}/ga/linux/x64/jdk/hotspot/normal/eclipse" \
         | tar -xz -C /opt/java --strip-components=1
ENV JAVA_HOME=/opt/java
ENV PATH=/opt/java/bin:${PATH}

ENV ANDROID_HOME=/opt/android-sdk
ENV ANDROID_SDK_ROOT=/opt/android-sdk
RUN mkdir -p ${ANDROID_HOME}/cmdline-tools \
    && curl -fsSL "https://dl.google.com/android/repository/commandlinetools-linux-${ANDROID_CMDLINE_TOOLS}_latest.zip" -o /tmp/cmdline-tools.zip \
    && unzip -q /tmp/cmdline-tools.zip -d ${ANDROID_HOME}/cmdline-tools \
    && mv ${ANDROID_HOME}/cmdline-tools/cmdline-tools ${ANDROID_HOME}/cmdline-tools/latest \
    && rm /tmp/cmdline-tools.zip \
    && yes | ${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager --licenses >/dev/null \
    && ${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager \
         "platform-tools" "platforms;android-${ANDROID_PLATFORM}" "build-tools;${ANDROID_BUILD_TOOLS}" "ndk;${ANDROID_NDK}"
ENV ANDROID_NDK_HOME=${ANDROID_HOME}/ndk/${ANDROID_NDK}
ENV PATH=${ANDROID_HOME}/cmdline-tools/latest/bin:${ANDROID_HOME}/platform-tools:${PATH}

RUN rustup target add aarch64-linux-android x86_64-linux-android

# `quench build apk|deb|rpm|arch`, from the registry. The index and token are the base image's cargo configuration.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo install --locked --registry ennor quench-tool

# A job's checkout is not owned by the user the pod runs as; git refuses such a directory without this.
RUN git config --system --add safe.directory '*'
