# Shared Dockerfile for all testing-grounds projects.
#
# Build args:
#   APP_NAME       - cargo binary name and resulting image identifier
#   APP_SOURCE_DIR - path from the Atlas repo root to the Rust source tree
#                    (workspace root or single-package dir, e.g.
#                     testing-grounds/microbenchmarks-async/rust)
#   CARGO_FEATURES - optional comma-separated Cargo feature list. When non-empty the
#                    build becomes `--no-default-features --features <value>`; when
#                    empty (the default) the cargo invocation is exactly what it was
#                    before this arg existed. Used by projects whose binary selects a
#                    compile-time variant, e.g. crud_perf's EXECUTOR_VARIANT.
#                    NOTE: the caller must also vary the image tag with the value —
#                    otherwise a cached image built for another variant is reused.
#
# The build context must be the Atlas repo root so that Atlas/ and febft/
# are available alongside the project source.
#
# Example (from testing-grounds/):
#   docker build \
#     --build-arg APP_NAME=microbenchmarks-async \
#     --build-arg APP_SOURCE_DIR=testing-grounds/microbenchmarks-async/rust \
#     -f bench/Dockerfile \
#     -t microbenchmarks \
#     ..
#
# Example with a feature-selected variant (from testing-grounds/):
#   docker build \
#     --build-arg APP_NAME=crud_perf_exec \
#     --build-arg APP_SOURCE_DIR=testing-grounds/crud_perf/crud_perf_exec \
#     --build-arg CARGO_FEATURES=baseline \
#     -f bench/Dockerfile \
#     -t crud-perf-baseline \
#     ..

# ── Build stage ───────────────────────────────────────────────────────────────────
FROM rustlang/rust:nightly-bullseye-slim AS build

ARG APP_NAME=microbenchmarks-async
ARG APP_SOURCE_DIR=testing-grounds/microbenchmarks-async/rust

WORKDIR /app

RUN apt-get update && \
    apt-get install -y --no-install-recommends capnproto clang && \
    rm -rf /var/lib/apt/lists/*

COPY Atlas ./Atlas
COPY febft ./febft
COPY ${APP_SOURCE_DIR} ./${APP_SOURCE_DIR}

# Declared here rather than beside the other ARGs so that changing it cannot invalidate
# the apt layer above — only the cargo build below has to re-run.
ARG CARGO_FEATURES=

# CARGO_TARGET_DIR redirects build output to a fixed path so the cache mount
# hits the same location regardless of APP_SOURCE_DIR.
RUN --mount=type=cache,target=/root/.rustup/toolchains \
    --mount=type=cache,target=/root/.rustup/downloads \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/cargo-target \
    set -e && \
    cd ${APP_SOURCE_DIR} && \
    CARGO_TARGET_DIR=/cargo-target \
        cargo build --locked --release \
                    --bin ${APP_NAME} \
                    ${CARGO_FEATURES:+--no-default-features --features ${CARGO_FEATURES}} \
                    --config 'profile.release.lto="thin"' && \
    cp /cargo-target/release/${APP_NAME} /bin/server && \
    strip /bin/server

# ── Runtime stage (WAN variant) ───────────────────────────────────────────────────
# Selected only when WAN_ENABLED=1 (see bench/bench.env). Unlike the distroless
# stage below it carries a shell and iproute2, so the entrypoint can install
# tc/netem rules in its own network namespace before exec'ing the server.
#
# bullseye-slim deliberately matches the glibc of the rustlang/rust:nightly-bullseye-slim
# build stage. Containers using this stage need cap_add: [NET_ADMIN].
#
# NOTE: this stage is intentionally placed *before* `final`. Both `docker build`
# without --target and Compose's `build:` block default to the LAST stage in the
# file, so keeping `final` last is what makes the non-WAN path behave exactly as
# it did before this stage existed.
FROM debian:bullseye-slim AS final-wan

RUN apt-get update && \
    apt-get install -y --no-install-recommends iproute2 iputils-ping && \
    rm -rf /var/lib/apt/lists/*

COPY --from=build /bin/server /usr/app/server
COPY testing-grounds/bench/scripts/wan-entrypoint.sh /usr/local/bin/wan-entrypoint.sh

EXPOSE 10000
WORKDIR /usr/app
ENTRYPOINT ["/usr/local/bin/wan-entrypoint.sh"]
CMD ["./server"]

# ── Runtime stage ─────────────────────────────────────────────────────────────────
# distroless/cc contains only glibc + libgcc — exactly what these binaries need.
FROM gcr.io/distroless/cc-debian11 AS final

COPY --from=build /bin/server /usr/app/server

EXPOSE 10000
WORKDIR /usr/app
CMD ["./server"]
