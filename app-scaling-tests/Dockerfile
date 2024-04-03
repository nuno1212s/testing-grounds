# syntax=docker/dockerfile:1

# Comments are provided throughout this file to help you get started.
# If you need more help, visit the Dockerfile reference guide at
# https://docs.docker.com/engine/reference/builder/

################################################################################
# Create a stage for building the application.

ARG RUST_VERSION=nightly-2024-02-03
ARG APP_NAME=app-scaling-tests
FROM rustlang/rust:nightly-bullseye-slim AS build
ARG APP_NAME
WORKDIR /app

ARG request_size
ARG reply_size
ARG state_size

ENV REQUEST_SIZE $request_size
ENV REPLY_SIZE $reply_size
ENV STATE_SIZE $state_size
ENV VERBOSE false

COPY Atlas ./Atlas
COPY febft ./febft
COPY testing-grounds/app-scaling-tests/rust ./testing-grounds/app-scaling-tests/rust

RUN apt update && apt install -y capnproto clang && rm -rf /var/lib/apt/lists/*

# Build the application.
# Leverage a cache mount to /usr/local/cargo/registry/
# for downloaded dependencies and a cache mount to /app/target/ for 
# compiled dependencies which will speed up subsequent builds.
# Leverage a bind mount to the src directory to avoid having to copy the
# source code into the container. Once built, copy the executable to an
# output directory before the cache mounted /app/target is unmounted.
RUN --mount=type=cache,target=/app/testing-grounds/app-scaling-tests/rust/target/ \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    <<EOF
cd testing-grounds/app-scaling-tests/rust
set -e
cargo build --locked --release
cp ./target/release/$APP_NAME /bin/server
EOF

################################################################################
# Create a new stage for running the application that contains the minimal
# runtime dependencies for the application. This often uses a different base
# image from the build stage where the necessary files are copied from the build
# stage.
#
# The example below uses the debian bullseye image as the foundation for running the app.
# By specifying the "bullseye-slim" tag, it will also use whatever happens to be the
# most recent version of that tag when you build your Dockerfile. If
# reproducability is important, consider using a digest
# (e.g., debian@sha256:ac707220fbd7b67fc19b112cee8170b41a9e97f703f588b2cdbbcdcecdd8af57).
ENV DEBIAN_FRONTEND=noninteractive
FROM debian:bullseye-slim AS final

# Create a non-privileged user that the app will run under.
# See https://docs.docker.com/develop/develop-images/dockerfile_best-practices/#user
ARG UID=10001
RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    appuser
USER appuser

# Copy the executable from the "build" stage.
COPY --from=build /bin/server /bin/

# Expose the port that the application listens on.
EXPOSE 10000

# What the container should run when it is started.
CMD ["/bin/server"]
