#!/usr/bin/env bash
# metrics-up.sh
#
# Brings the metrics stack (bench/grafana/docker-compose.yml — InfluxDB + Grafana) up
# and does not return until InfluxDB is actually answering queries.
#
# This is the single implementation behind both `make metrics` and the automatic
# start that `local`, `remote-docker` and `remote-bare` perform before a run, so the
# explicit and the implicit paths cannot drift.
#
# Why it blocks rather than just starting containers: a node whose InfluxDB refuses
# the connection does not degrade, it dies. Atlas' OS monitor thread writes every
# 250 ms through `rt::block_on(client.query(..)).expect("Failed to write metrics to
# influxdb")` (Atlas/Atlas-Metrics/src/metrics/os_mon.rs), and the release profile
# sets `panic = "abort"` — which is the profile the bench Dockerfile and
# `make build-binary` both use. So a replica started a second too early aborts
# instead of running without OS metrics.
#
# Everything here is idempotent: `up -d` on a running stack is a no-op, and the
# database/retention statements are safe to repeat, so putting this in front of every
# run costs a fraction of a second once the stack is up.
#
# Usage: metrics-up.sh [local|remote|explicit]
#   The mode only selects which warnings are relevant; it changes nothing else.
#
# Environment (exported by the shared bench Makefile):
#   GLOBAL_BENCH_DIR, GENERATED, DOCKER, COMPOSE
#   LOCAL_INFLUXDB, INFLUXDB_RETENTION, GRAFANA_BIND, GRAFANA_PORT, INFLUXDB_BIND
#   METRICS_WAIT_TIMEOUT - seconds to wait for a cold InfluxDB (default 90)

set -euo pipefail

: "${GLOBAL_BENCH_DIR:?GLOBAL_BENCH_DIR not set}"
: "${GENERATED:?GENERATED not set}"
: "${DOCKER:=docker}"
: "${COMPOSE:=docker compose}"
: "${LOCAL_INFLUXDB:=1}"
: "${INFLUXDB_RETENTION:=}"
: "${INFLUXDB_BIND:=127.0.0.1}"
: "${GRAFANA_BIND:=127.0.0.1}"
: "${GRAFANA_PORT:=3000}"

MODE="${1:-explicit}"

COMPOSE_FILE="$GLOBAL_BENCH_DIR/grafana/docker-compose.yml"
PROJECT="atlas-grafana"
INFLUX_CONTAINER="atlas-influxdb"
# Seconds to wait for a cold InfluxDB. A first start on an empty volume creates the
# database and the admin user before it begins serving, which is the slow case.
: "${METRICS_WAIT_TIMEOUT:=90}"
WAIT_TIMEOUT="$METRICS_WAIT_TIMEOUT"

# COMPOSE may be a two-word command ("docker compose"); it is deliberately unquoted
# so it word-splits.
# shellcheck disable=SC2086
compose() { $COMPOSE "$@"; }

# ── 1. Resolve the target and write the provisioning ──────────────────────────────
bash "$GLOBAL_BENCH_DIR/scripts/gen-grafana.sh"

# ── 2. Start the stack ────────────────────────────────────────────────────────────
# The influxdb service sits behind the `local-influx` Compose profile, so
# LOCAL_INFLUXDB=0 starts Grafana alone rather than a redundant second server.
PROFILE_ARGS=()
if [ "$LOCAL_INFLUXDB" = "1" ]; then
    PROFILE_ARGS=(--profile local-influx)
fi

compose "${PROFILE_ARGS[@]}" -p "$PROJECT" -f "$COMPOSE_FILE" up -d

# ── 3. Make sure the database will accept writes ──────────────────────────────────
# Read a value out of the running container's environment rather than re-parsing the
# TOML here: it is the same generated influxdb.env the server itself booted with, so
# the credentials used to probe are by construction the credentials it accepts.
printenv_in_container() {
    $DOCKER exec "$INFLUX_CONTAINER" printenv "$1" 2>/dev/null || true
}

influx_exec() {
    $DOCKER exec "$INFLUX_CONTAINER" influx \
        -username "$INFLUX_ADMIN_USER" \
        -password "$INFLUX_ADMIN_PASSWORD" \
        -execute "$1"
}

if [ "$LOCAL_INFLUXDB" = "1" ]; then
    DB_NAME="$(printenv_in_container INFLUXDB_DB)"
    : "${DB_NAME:=atlas}"
    INFLUX_ADMIN_USER="$(printenv_in_container INFLUXDB_ADMIN_USER)"
    INFLUX_ADMIN_PASSWORD="$(printenv_in_container INFLUXDB_ADMIN_PASSWORD)"

    printf 'Waiting for InfluxDB'
    waited=0
    until influx_exec 'SHOW DATABASES' >/dev/null 2>&1; do
        if [ "$waited" -ge "$WAIT_TIMEOUT" ]; then
            echo ""
            echo "ERROR: InfluxDB did not become ready within ${WAIT_TIMEOUT}s." >&2
            echo "       Inspect it with:" >&2
            echo "           $DOCKER logs $INFLUX_CONTAINER" >&2
            echo "       Starting a run now would abort every node (see the header of" >&2
            echo "       this script), so the run is not being started." >&2
            exit 1
        fi
        printf '.'
        sleep 1
        waited=$((waited + 1))
    done
    echo " ready (${waited}s)"

    # CREATE DATABASE is idempotent, and needed beyond the image's INFLUXDB_DB: that
    # only runs on an empty volume, so a volume that predates a db_name change (or was
    # created before this file existed) would otherwise have no matching database and
    # every write would 404.
    influx_exec "CREATE DATABASE \"$DB_NAME\"" >/dev/null

    if [ -n "$INFLUXDB_RETENTION" ]; then
        influx_exec "ALTER RETENTION POLICY autogen ON \"$DB_NAME\" DURATION $INFLUXDB_RETENTION DEFAULT" >/dev/null
        echo "InfluxDB: database '$DB_NAME', retention $INFLUXDB_RETENTION"
    else
        echo "InfluxDB: database '$DB_NAME', retention unlimited (set INFLUXDB_RETENTION to bound it)"
    fi

    if [ "$MODE" = "remote" ]; then
        # Remote nodes read the *unmodified* influx_db.toml — only gen-local-compose.sh
        # rewrites the address to the in-network alias — so this local server is only
        # the right target if that file names a host:port this cluster can reach.
        echo ""
        echo "  NOTE: remote deployment. The nodes write to whatever"
        echo "        config-base/common/influx_db.toml names; they cannot resolve the"
        echo "        'influxdb' alias, which only exists on atlas_network. For them to"
        echo "        reach this container, influx_db.toml must point at this host and"
        echo "        the port must be published beyond loopback:"
        echo ""
        echo "            make <project> $MODE INFLUXDB_BIND=0.0.0.0"
        echo ""
        echo "        (currently INFLUXDB_BIND=$INFLUXDB_BIND)"
    fi
else
    # Nothing to wait for — but the failure mode is a silently aborting cluster, so
    # probe the external instance and say so rather than letting the run discover it.
    INFLUX_URL="$(sed -n "s/^INFLUX_URL='\(.*\)'$/\1/p" "$GENERATED/grafana/grafana.env" | head -n 1)"
    if ! python3 - "$INFLUX_URL" <<'PY'
import sys, urllib.error, urllib.request

url = sys.argv[1].rstrip("/") + "/ping"
try:
    urllib.request.urlopen(url, timeout=3)
except urllib.error.HTTPError:
    # An HTTP status back means something is listening and speaking HTTP, which is
    # the whole question here — /ping's own 204 is not worth being fussy about.
    pass
except Exception:
    sys.exit(1)
PY
    then
        echo ""
        echo "  WARNING: LOCAL_INFLUXDB=0 and $INFLUX_URL is not answering."
        echo "           Every node aborts on its first failed metrics write, so this"
        echo "           run will not survive. Either start that instance, or use the"
        echo "           stack's own database with LOCAL_INFLUXDB=1."
    fi
fi

GRAFANA_HOST="$GRAFANA_BIND"
[ "$GRAFANA_HOST" = "0.0.0.0" ] && GRAFANA_HOST="localhost"
echo "Grafana → http://$GRAFANA_HOST:$GRAFANA_PORT  (folder 'Atlas')"
