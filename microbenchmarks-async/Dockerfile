# ── Build stage ──────────────────────────────────────────────────────────────────
FROM rustlang/rust:nightly-bullseye-slim AS build

ARG APP_NAME=microbenchmarks-async

WORKDIR /app

RUN apt-get update && \
    apt-get install -y --no-install-recommends capnproto clang && \
    rm -rf /var/lib/apt/lists/*

COPY Atlas ./Atlas
COPY febft ./febft
COPY testing-grounds/microbenchmarks-async/rust ./testing-grounds/microbenchmarks-async/rust

# Cache the toolchain download and cargo registry across builds.
# The toolchain is determined by rust-toolchain.toml in the project directory.
RUN --mount=type=cache,target=/root/.rustup/toolchains \
    --mount=type=cache,target=/root/.rustup/downloads \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/testing-grounds/microbenchmarks-async/rust/target \
    set -e && \
    cd testing-grounds/microbenchmarks-async/rust && \
    cargo build --locked --release --config 'profile.release.lto="thin"' && \
    cp target/release/${APP_NAME} /bin/server && \
    strip /bin/server

# ── Runtime stage ─────────────────────────────────────────────────────────────────
# distroless/cc contains only glibc + libgcc — exactly what this binary needs.
FROM gcr.io/distroless/cc-debian11 AS final

COPY --from=build /bin/server /usr/app/server

EXPOSE 10000
WORKDIR /usr/app
CMD ["./server"]
