# syntax=docker/dockerfile:1

# ---------- 1. 前端构建 ----------
FROM node:24-bookworm-slim AS frontend
WORKDIR /src/frontend

# 先只拷贝依赖清单，命中缓存时跳过 npm ci
COPY frontend/package.json frontend/package-lock.json frontend/.npmrc ./
RUN --mount=type=cache,target=/root/.npm npm ci

COPY frontend/ ./
RUN npm run build

# ---------- 2. 后端构建 ----------
FROM rust:1.99-bookworm AS backend
WORKDIR /src

ENV CARGO_NET_RETRY=5 \
    CARGO_TERM_COLOR=never \
    CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse \
    CARGO_REGISTRIES_CRATES_IO_INDEX=sparse+https://rsproxy.cn/index/ \
    CARGO_NET_GIT_FETCH_WITH_CLI=false

# 依赖层：先用空壳源码编译依赖，源码变化时无需重编全部 crate
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY .cargo/config.toml .cargo/config.toml
RUN mkdir -p src \
    && echo 'fn main() {}' > src/main.rs \
    && echo '' > src/lib.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --bin snack-warehouse-backend \
 && rm -rf src

COPY backend/src ./src

# 前端产物直接作为后端静态目录（二进制同目录 static/）
COPY --from=frontend /src/frontend/dist/frontend/browser ./static

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --bin snack-warehouse-backend \
 && install -m 755 target/release/snack-warehouse-backend /src/snack-warehouse-backend

# ---------- 3. 运行时 ----------
FROM debian:bookworm-slim AS runtime

LABEL org.opencontainers.image.title="snack-warehouse" \
      org.opencontainers.image.description="零食仓库管理平台（Rust + axum + SQLite + Angular）" \
      org.opencontainers.image.source="https://github.com/kcgp007/snack-warehouse-management"

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl tzdata \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --gid 10001 app \
 && useradd --uid 10001 --gid app --no-create-home --home-dir /app --shell /usr/sbin/nologin app

WORKDIR /app

COPY --from=backend /src/snack-warehouse-backend /app/snack-warehouse-backend
COPY --from=backend /src/static /app/static

RUN mkdir -p /app/data && chown -R app:app /app/data

ENV TZ=Asia/Shanghai
EXPOSE 3000
STOPSIGNAL SIGTERM

# 容器内自检：自签证书用 -k，失败即 unhealthy
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD curl -kfsS https://127.0.0.1:3000/ -o /dev/null || exit 1

USER 10001:10001
CMD ["/app/snack-warehouse-backend"]
