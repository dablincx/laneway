# syntax=docker/dockerfile:1
# Dev container: Rust + Node toolchains. Source is bind-mounted, caches live in volumes.
FROM rust:1-trixie

ARG NODE_MAJOR=22
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl git git-lfs mold clang sudo \
    && curl -fsSL https://deb.nodesource.com/setup_${NODE_MAJOR}.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/* \
    && corepack enable

RUN rustup component add rustfmt clippy \
    && curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
    && cargo binstall -y --locked cargo-nextest cargo-watch cargo-deny sqlx-cli sccache git-cliff \
    && mv /usr/local/cargo/bin/* /usr/local/bin/ \
    && chmod -R a+rwX /usr/local/cargo /usr/local/rustup

# Files created in the container belong to the host user (dev.sh passes UID/GID).
ARG UID=1000
ARG GID=1000
RUN groupadd --gid ${GID} dev 2>/dev/null || true \
    && useradd --uid ${UID} --gid ${GID} --create-home --shell /bin/bash dev 2>/dev/null || true \
    && echo "dev ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/dev

ENV CARGO_HOME=/cache/cargo \
    CARGO_TARGET_DIR=/cache/target \
    RUSTC_WRAPPER=sccache \
    SCCACHE_DIR=/cache/sccache \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=mold" \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=clang \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=mold" \
    COREPACK_ENABLE_DOWNLOAD_PROMPT=0
WORKDIR /workspace
