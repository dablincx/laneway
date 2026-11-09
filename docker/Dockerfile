# syntax=docker/dockerfile:1
# Production image: one binary, two roles (LANEWAY_PROCESS_ROLE=app|worker|all).

ARG RUST_VERSION=1
ARG NODE_VERSION=22

# --- Rust: cache dependencies with cargo-chef ---
FROM rust:${RUST_VERSION}-slim-trixie AS chef
RUN cargo install cargo-chef --locked
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS server
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked -p laneway-server \
    && cp target/release/laneway /laneway

# --- Web UI ---
FROM node:${NODE_VERSION}-trixie-slim AS web
RUN corepack enable
WORKDIR /src/web
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY locales /src/locales
COPY web .
RUN pnpm build

# --- Runtime ---
FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git git-lfs tini \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin laneway \
    && mkdir -p /data && chown laneway:laneway /data
COPY --from=server /laneway /usr/local/bin/laneway
COPY --from=web /src/web/dist /app/web
COPY locales /app/locales

USER laneway
VOLUME /data
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s CMD ["laneway", "health"]
ENTRYPOINT ["tini", "--", "laneway"]
CMD ["serve"]
