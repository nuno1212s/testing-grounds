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
#   BINARY_NAME, APP_SOURCE_DIR, N_REPLICAS, N_CLIENTS, N_CLIENT_MACHINES, RUST_LOG
#   LOCAL_INFLUXDB   - 1 to point the generated configs at the metrics stack's own
#                      InfluxDB. The server itself is NOT emitted here: it lives in
#                      bench/grafana/docker-compose.yml, because a database that is
#                      torn down with the run is empty exactly when you want to read
#                      it. All this script does is rewrite the address.
#   INFLUX_EXTRA     - optional run name; becomes the `extra` tag on every metric
#                      point (config crate reads INFLUX_* into the influx config).
#                      Emitted only when non-empty: an empty value would replace
#                      the "None" default with an empty tag.
#
# WAN emulation (optional, see bench/wan-profiles/ and bench/README.md):
#   WAN_ENABLED=1 additionally emits, per shaped node, cap_add: [NET_ADMIN], a
#   static ipv4_address, the /wan spec mount and WAN_NODE/WAN_IFACE, and selects
#   the `final-wan` build target. With WAN_ENABLED=0 (the default) the output is
#   byte-identical to what this script produced before WAN support existed.
#
#   WAN_SUBNET        - subnet for static addressing (default 10.90.0.0/24)
#   WAN_IFACE         - interface shaped inside the container (default eth0)
#   WAN_SHAPE_CLIENTS - 0 to shape replica<->replica links only
#   IMAGE_NAME_EFFECTIVE - image tag to use (Makefile sets <IMAGE_NAME>-wan in WAN mode)

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
: "${N_CLIENT_MACHINES:?N_CLIENT_MACHINES not set}"
: "${RUST_LOG:=INFO}"
: "${LOCAL_INFLUXDB:=1}"
: "${WAN_ENABLED:=0}"
: "${INFLUX_EXTRA:=}"
: "${WAN_SUBNET:=10.90.0.0/24}"
: "${WAN_IFACE:=eth0}"
: "${WAN_SHAPE_CLIENTS:=1}"

IMAGE="${IMAGE_NAME_EFFECTIVE:-$IMAGE_NAME}"

OUT="$GENERATED/local-compose.yml"
mkdir -p "$GENERATED/logs"

_sed_i() { if [ "$(uname)" = "Darwin" ]; then sed -i '' "$@"; else sed -i "$@"; fi; }

# Static address for a node, by offset within WAN_SUBNET. Offsets match gen-wan.py:
# replica-i .10+i, client-i .100+i. The metrics stack is not addressed statically —
# like Grafana, InfluxDB takes a dynamic address at the low end of the subnet, well
# clear of these offsets, and is left unshaped.
_wan_ip() {
  python3 -c "import ipaddress,sys; print(ipaddress.ip_network(sys.argv[1])[int(sys.argv[2])])" \
    "$WAN_SUBNET" "$1"
}

if [ "$WAN_ENABLED" = "1" ]; then
  mkdir -p "$GENERATED/wan"
fi

# Point the generated configs at the metrics stack's InfluxDB, reachable on
# atlas_network under the `influxdb` alias. Only the address is rewritten — db_name,
# user and password stay as config-base names them, and gen-grafana.sh feeds the same
# three to the server and to Grafana.
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

  CLI_BASE=1000

  # ── Replicas ──────────────────────────────────────────────────────────────────
  for i in $(seq 0 $((N_REPLICAS - 1))); do
    HOST_PORT=$((10000 + i))
    cat <<EOF
  replica-${i}:
    image: ${IMAGE}
    build:
      context: ${BUILD_CTX_ABS}
      dockerfile: ${DOCKERFILE_ABS}
EOF
    [ "$WAN_ENABLED" = "1" ] && echo "      target: final-wan"
    cat <<EOF
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
EOF
    [ "$WAN_ENABLED" = "1" ] && echo "      - ${GENERATED}/wan:/wan:ro"
    cat <<EOF
    environment:
      ID: ${i}
      OWN_NODE__NODE_ID: ${i}
      OWN_NODE__IP: "replica-${i}"
      OWN_NODE__HOSTNAME: "srv${i}"
      OWN_NODE__NODE_TYPE: "Replica"
      RUST_LOG: "${RUST_LOG}"
      RUST_BACKTRACE: full
EOF
    [ -n "$INFLUX_EXTRA" ] && echo "      INFLUX_EXTRA: \"${INFLUX_EXTRA}\""
    if [ "$WAN_ENABLED" = "1" ]; then
      cat <<EOF
      WAN_NODE: "replica-${i}"
      WAN_IFACE: "${WAN_IFACE}"
    cap_add:
      - NET_ADMIN
EOF
    fi
    cat <<EOF
    restart: on-failure
    networks:
      atlas_network:
        aliases:
          - replica-${i}
EOF
    [ "$WAN_ENABLED" = "1" ] && echo "        ipv4_address: $(_wan_ip $((10 + i)))"
  done

  # ── Clients ───────────────────────────────────────────────────────────────────
  # One container per client machine; each container runs N_CLIENTS logical clients.
  for i in $(seq 0 $((N_CLIENT_MACHINES - 1))); do
    NODE_ID=$((CLI_BASE + i))
    HOST_PORT=$((11000 + i))
    # Clients are shaped only when WAN_SHAPE_CLIENTS=1; they still need a static
    # address either way, so the replicas' destination-IP filters keep matching.
    CLIENT_SHAPED=0
    if [ "$WAN_ENABLED" = "1" ] && [ "$WAN_SHAPE_CLIENTS" = "1" ]; then CLIENT_SHAPED=1; fi
    cat <<EOF
  client-${i}:
    image: ${IMAGE}
    build:
      context: ${BUILD_CTX_ABS}
      dockerfile: ${DOCKERFILE_ABS}
EOF
    [ "$WAN_ENABLED" = "1" ] && echo "      target: final-wan"
    cat <<EOF
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
EOF
    [ "$CLIENT_SHAPED" = "1" ] && echo "      - ${GENERATED}/wan:/wan:ro"
    cat <<EOF
    environment:
      ID: ${NODE_ID}
      OWN_NODE__NODE_ID: ${NODE_ID}
      OWN_NODE__IP: "client-${i}"
      OWN_NODE__HOSTNAME: "cli${NODE_ID}"
      OWN_NODE__NODE_TYPE: "Client"
      CLIENT: 1
      RUST_LOG: "${RUST_LOG}"
      RUST_BACKTRACE: full
EOF
    [ -n "$INFLUX_EXTRA" ] && echo "      INFLUX_EXTRA: \"${INFLUX_EXTRA}\""
    if [ "$CLIENT_SHAPED" = "1" ]; then
      cat <<EOF
      WAN_NODE: "client-${i}"
      WAN_IFACE: "${WAN_IFACE}"
    cap_add:
      - NET_ADMIN
EOF
    fi
    cat <<EOF
    restart: on-failure
    networks:
      atlas_network:
        aliases:
          - client-${i}
EOF
    [ "$WAN_ENABLED" = "1" ] && echo "        ipv4_address: $(_wan_ip $((100 + i)))"
  done

  cat <<'NETFOOTER'
networks:
  atlas_network:
    external: true
NETFOOTER

} > "$OUT"

if [ "$WAN_ENABLED" = "1" ]; then
  echo "Written $OUT ($N_REPLICAS replicas, $N_CLIENT_MACHINES client containers, $N_CLIENTS logical clients each) [WAN: $WAN_SUBNET, image $IMAGE]"
else
  echo "Written $OUT ($N_REPLICAS replicas, $N_CLIENT_MACHINES client containers, $N_CLIENTS logical clients each)"
fi
