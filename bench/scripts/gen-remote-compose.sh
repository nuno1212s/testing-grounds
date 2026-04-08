#!/usr/bin/env bash
# gen-remote-compose.sh
#
# Generates per-machine docker-compose files in generated/per-machine/.
# All variables are read from the environment (exported by the shared bench Makefile):
#
#   BENCH_DIR          - absolute path to project bench dir (for hosts.yml)
#   GENERATED          - absolute path to shared generated/ dir
#   N_CLIENTS, N_CLIENT_MACHINES, DOCKER_IMAGE, DOCKER_VERSION, RUST_LOG

set -euo pipefail

: "${BENCH_DIR:?BENCH_DIR not set}"
: "${GENERATED:?GENERATED not set}"
: "${N_CLIENTS:?N_CLIENTS not set}"
: "${N_CLIENT_MACHINES:?N_CLIENT_MACHINES not set}"
: "${DOCKER_IMAGE:?DOCKER_IMAGE not set}"
: "${DOCKER_VERSION:?DOCKER_VERSION not set}"
: "${RUST_LOG:=INFO}"

HOSTS_YML="$BENCH_DIR/hosts.yml"
OUT_DIR="$GENERATED/per-machine"

mkdir -p "$OUT_DIR"

python3 - <<PYEOF
import yaml, sys, os

n_clients = int("$N_CLIENTS")
n_client_machines = int("$N_CLIENT_MACHINES")
image = "$DOCKER_IMAGE:$DOCKER_VERSION"
rust_log = "$RUST_LOG"
out_dir = "$OUT_DIR"
cli_base = 1000

with open("$HOSTS_YML") as f:
    inv = yaml.safe_load(f)

# ── Replica compose files ─────────────────────────────────────────────────────────
for hostname, vals in inv["replicas"]["hosts"].items():
    nid = vals["node_id"]
    ip = vals["machine_ip"]
    content = f"""services:
  replica-{nid}:
    image: {image}
    ports:
      - "10000:10000"
    volumes:
      - ./config:/usr/app/config
      - ./ca-root:/usr/app/ca-root
      - ./logs:/usr/app/logs
    environment:
      ID: {nid}
      OWN_NODE__NODE_ID: {nid}
      OWN_NODE__IP: "{ip}"
      OWN_NODE__HOSTNAME: "srv{nid}"
      OWN_NODE__NODE_TYPE: "Replica"
      RUST_LOG: "{rust_log}"
      RUST_BACKTRACE: full
    restart: unless-stopped
"""
    path = os.path.join(out_dir, f"{hostname}-compose.yml")
    with open(path, "w") as f:
        f.write(content)
    print(f"  {path}")

# ── Client compose files ──────────────────────────────────────────────────────────
client_machines = list(inv["clients"]["hosts"].items())
if n_client_machines > len(client_machines):
    print(f"ERROR: N_CLIENT_MACHINES={n_client_machines} exceeds client hosts in hosts.yml ({len(client_machines)})", file=sys.stderr)
    sys.exit(1)
active_machines = client_machines[:n_client_machines]

machine_clients = {hostname: [] for hostname, _ in active_machines}
for i in range(n_clients):
    nid = cli_base + i
    hostname, vals = active_machines[i % n_client_machines]
    machine_ip = vals["machine_ip"]
    machine_clients[hostname].append((i, nid, machine_ip))

for hostname, clients in machine_clients.items():
    services = ""
    for idx, nid, ip in clients:
        host_port = 10000 + nid
        services += f"""  client-{idx}:
    image: {image}
    ports:
      - "{host_port}:10000"
    volumes:
      - ./config:/usr/app/config
      - ./ca-root:/usr/app/ca-root
      - ./logs:/usr/app/logs
    environment:
      ID: {nid}
      OWN_NODE__NODE_ID: {nid}
      OWN_NODE__IP: "{ip}"
      OWN_NODE__HOSTNAME: "cli{nid}"
      OWN_NODE__NODE_TYPE: "Client"
      CLIENT: 1
      RUST_LOG: "{rust_log}"
      RUST_BACKTRACE: full
    restart: "no"
"""
    content = f"services:\n{services}"
    path = os.path.join(out_dir, f"{hostname}-compose.yml")
    with open(path, "w") as f:
        f.write(content)
    print(f"  {path} ({len(clients)} client(s))")

print(f"Generated {len(inv['replicas']['hosts'])} replica + {n_client_machines} client compose files in {out_dir}/")
PYEOF
