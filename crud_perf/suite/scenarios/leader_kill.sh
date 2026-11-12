#!/bin/bash
# Stops the leader (replica-0) mid-run and checks that the survivors view-change and keep serving.
# Needs ~3 minutes and no other cluster running. Usage: scenarios/leader_kill.sh
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
TG=$(cd "$HERE/../../.." && pwd)
OUT=${OUT:-$HERE/../results/leader_kill.out}
mkdir -p "$(dirname "$OUT")"; : > "$OUT"
log() { echo "[$(date +%T)] $*" >> "$OUT"; }

[ -d "$TG/bench/generated/logs" ] && mv "$TG/bench/generated/logs" "$TG/bench/generated/logs.leader_kill.$(date +%H%M%S)"

ARGS="N_REPLICAS=4 N_CLIENT_MACHINES=1 N_CLIENTS=5 CONCURRENT_RQS=3 WARMUP_SECS=3 MEASURE_SECS=40 \
CLIENT_REQUEST_METRICS=0 RUST_LOG=info WORKLOAD_TYPE=uniform_cheap FORCE_ORDERED=true KEY_DISTRIBUTION=uniform \
KEY_SPACE_SIZE=10000 PRELOAD_KEYS=10000 READ_RATIO=70 CREATE_RATIO=15 UPDATE_RATIO=10 DELETE_RATIO=5 \
PROCESSING_MODE=spin WAN_ENABLED=0 WAN_SHAPE_CLIENTS=1 WAN_TIMEOUT_SCALE=1 PIN_CPUS=0 REPLICA_CORES=8 \
CLIENT_CORES=8 REQUEST_TIMEOUT_MICROS=1000000 CLIENT_FAULT_SENDS_TO=all EXECUTOR_VARIANT=crud_single \
INFLUX_EXTRA=leader_kill"
(setsid make -C "$TG" crud_perf local $ARGS > "$OUT.make.log" 2>&1 &)
log "cluster starting"

for _ in $(seq 1 120); do
  n=$(docker logs client-0 2>&1 | grep -c "PHASE measure_start"); [ "$n" -ge 5 ] && break; sleep 2
done
log "measure started ($n clients)"
sleep 8
docker stop replica-0 >/dev/null 2>&1 && log "replica-0 (leader) stopped"

for _ in $(seq 1 60); do
  r=$(docker logs client-0 2>&1 | grep -c 'RESULT {'); [ "$r" -ge 5 ] && break; sleep 2
done
log "RESULT lines seen: $r of 5"
docker logs client-0 2>&1 | grep -o 'RESULT {[^}]*}' | cut -c1-200 >> "$OUT"
make -C "$TG" crud_perf stop-local >/dev/null 2>&1
log "cluster stopped"
[ "$r" -ge 5 ] && echo PASS >> "$OUT" || echo FAIL >> "$OUT"
