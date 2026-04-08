#!/usr/bin/env bash
# gen-local-compose.sh
#
# Generates generated/local-compose.yml for local Docker Compose deployment.
# All variables are read from the environment (exported by the shared bench Makefile):
#
#   BENCH_DIR        - absolute path to project bench dir (for bench.env paths)
#   GENERATED        - absolute path to shared generated/ dir
#   GLOBAL_BENCH_DIR - absolute path to testing-grounds/bench
#   IMAGE_NAME       - Docker image tag to build/use locally
#   BUILD_CTX_ABS    - absolute path to Docker build context (Atlas repo root)
#   DOCKERFILE_ABS   - absolute path to the shared Dockerfile
#   BINARY_NAME, APP_SOURCE_DIR, N_REPLICAS, N_CLIENTS, RUST_LOG, LOCAL_INFLUXDB

set -euo pipefail

: "${BENCH_DIR:?BENCH_DIR not set}"
: "${GENERATED:?GENERATED not set}"
: "${GLOBAL_BENCH_DIR:?GLOBAL_BENCH_DIR not set}"
: "${IMAGE_NAME:?IMAGE_NAME not set}"
: "${BUILD_CTX_ABS:?BUILD_CTX_ABS not set}"
: "${DOCKERFILE_ABS:?DOCKERFILE_ABS not set}"
: "${BINARY_NAME:?BINARY_NAME not set}"
: "${APP_SOURCE_DIR:?APP_SOURCE_DIR not set}"
: "${N_REPLICAS:?N_REPLICAS not set}"
: "${N_CLIENTS:?N_CLIENTS not set}"
: "${RUST_LOG:=INFO}"
: "${LOCAL_INFLUXDB:=0}"

OUT="$GENERATED/local-compose.yml"
mkdir -p "$GENERATED/logs"

_sed_i() { if [ "$(uname)" = "Darwin" ]; then sed -i '' "$@"; else sed -i "$@"; fi; }

if [ "$LOCAL_INFLUXDB" = "1" ]; then
  for f in "$GENERATED/config-replicas/influx_db.toml" \
           "$GENERATED/config-clients/influx_db.toml"; do
    [ -f "$f" ] && _sed_i 's|ip = ".*"|ip = "http://influxdb:8086"|' "$f"
  done
fi

{
  cat <<'HEADER'
services:
HEADER

  if [ "$LOCAL_INFLUXDB" = "1" ]; then
    cat <<'INFLUX'
  influxdb:
    image: influxdb:1.8
    container_name: influxdb
    hostname: influxdb
    ports:
      - "8086:8086"
    environment:
      INFLUXDB_DB: atlas
      INFLUXDB_HTTP_AUTH_ENABLED: "false"
    volumes:
      - influxdb-data:/var/lib/influxdb
    networks:
      atlas_network:
        aliases:
          - influxdb
INFLUX
  fi

  CLI_BASE=1000

  # ── Replicas ──────────────────────────────────────────────────────────────────
  for i in $(seq 0 $((N_REPLICAS - 1))); do
    HOST_PORT=$((10000 + i))
    cat <<EOF
  replica-${i}:
    image: ${IMAGE_NAME}
    build:
      context: ${BUILD_CTX_ABS}
      dockerfile: ${DOCKERFILE_ABS}
      args:
        APP_NAME: ${BINARY_NAME}
        APP_SOURCE_DIR: ${APP_SOURCE_DIR}
    container_name: replica-${i}
    hostname: replica-${i}
    ports:
      - "${HOST_PORT}:10000"
    env_file:
      - ${GLOBAL_BENCH_DIR}/bench.env
      - ${BENCH_DIR}/bench.env
    volumes:
      - ${GENERATED}/config-replicas:/usr/app/config
      - ${GENERATED}/ca-root:/usr/app/ca-root
      - ${GENERATED}/logs:/usr/app/logs
    environment:
      ID: ${i}
      OWN_NODE__NODE_ID: ${i}
      OWN_NODE__IP: "replica-${i}"
      OWN_NODE__HOSTNAME: "srv${i}"
      OWN_NODE__NODE_TYPE: "Replica"
      RUST_LOG: "${RUST_LOG}"
      RUST_BACKTRACE: full
    restart: on-failure
    networks:
      atlas_network:
        aliases:
          - replica-${i}
EOF
  done

  # ── Clients ───────────────────────────────────────────────────────────────────
  for i in $(seq 0 $((N_CLIENTS - 1))); do
    NODE_ID=$((CLI_BASE + i))
    HOST_PORT=$((11000 + i))
    cat <<EOF
  client-${i}:
    image: ${IMAGE_NAME}
    build:
      context: ${BUILD_CTX_ABS}
      dockerfile: ${DOCKERFILE_ABS}
      args:
        APP_NAME: ${BINARY_NAME}
        APP_SOURCE_DIR: ${APP_SOURCE_DIR}
    container_name: client-${i}
    hostname: client-${i}
    ports:
      - "${HOST_PORT}:10000"
    env_file:
      - ${GLOBAL_BENCH_DIR}/bench.env
      - ${BENCH_DIR}/bench.env
    volumes:
      - ${GENERATED}/config-clients:/usr/app/config
      - ${GENERATED}/ca-root:/usr/app/ca-root
      - ${GENERATED}/logs:/usr/app/logs
    environment:
      ID: ${NODE_ID}
      OWN_NODE__NODE_ID: ${NODE_ID}
      OWN_NODE__IP: "client-${i}"
      OWN_NODE__HOSTNAME: "cli${NODE_ID}"
      OWN_NODE__NODE_TYPE: "Client"
      CLIENT: 1
      RUST_LOG: "${RUST_LOG}"
      RUST_BACKTRACE: full
    restart: on-failure
    networks:
      atlas_network:
        aliases:
          - client-${i}
EOF
  done

  cat <<'NETFOOTER'
networks:
  atlas_network:
    external: true
NETFOOTER

  if [ "$LOCAL_INFLUXDB" = "1" ]; then
    cat <<'VOLFOOTER'
volumes:
  influxdb-data:
VOLFOOTER
  fi
} > "$OUT"

echo "Written $OUT ($N_REPLICAS replicas, $N_CLIENTS clients)"
