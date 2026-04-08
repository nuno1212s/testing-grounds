#!/usr/bin/env bash
# gen-nodes-toml.sh <local|remote> <N_REPLICAS> <N_CLIENTS> <N_CLIENT_MACHINES>
#
# Generates generated/nodes.toml for the given deployment mode.
# - local:  uses Docker service names as IPs
# - remote: reads hosts.yml for IPs; distributes N_CLIENTS round-robin
#
# Reads from environment (exported by shared bench Makefile):
#   BENCH_DIR  - absolute path to project bench dir (for hosts.yml)
#   GENERATED  - absolute path to shared generated/ output dir
#
# Hostname convention must match ca-root cert names:
#   Replicas: srv0, srv1, ..., srv{N-1}
#   Clients:  cli1000, cli1001, ..., cli{1000+N-1}  (CLI_BASE=1000)

set -euo pipefail

: "${BENCH_DIR:?BENCH_DIR not set}"
: "${GENERATED:?GENERATED not set}"

MODE=$1
N_REPLICAS=$2
N_CLIENTS=$3
N_CLIENT_MACHINES=$4

HOSTS_YML="$BENCH_DIR/hosts.yml"
OUT="$GENERATED/nodes.toml"

mkdir -p "$(dirname "$OUT")"

python3 - <<PYEOF
import sys

mode = "$MODE"
n_replicas = int("$N_REPLICAS")
n_clients = int("$N_CLIENTS")
n_client_machines = int("$N_CLIENT_MACHINES")
cli_base = 1000

replica_entries = []
client_entries = []

if mode == "local":
    for i in range(n_replicas):
        replica_entries.append(
            f'    {{ node_id = {i}, ip = "replica-{i}", port = 10000, '
            f'hostname = "srv{i}", node_type = "Replica" }}'
        )
    for i in range(n_clients):
        nid = cli_base + i
        client_entries.append(
            f'    {{ node_id = {nid}, ip = "client-{i}", port = 10000, '
            f'hostname = "cli{nid}", node_type = "Client" }}'
        )

elif mode == "remote":
    import yaml
    with open("$HOSTS_YML") as f:
        inv = yaml.safe_load(f)

    for host, vals in inv["replicas"]["hosts"].items():
        nid = vals["node_id"]
        ip = vals["machine_ip"]
        replica_entries.append(
            f'    {{ node_id = {nid}, ip = "{ip}", port = 10000, '
            f'hostname = "srv{nid}", node_type = "Replica" }}'
        )

    client_machines = list(inv["clients"]["hosts"].values())
    if n_client_machines > len(client_machines):
        print(f"ERROR: N_CLIENT_MACHINES={n_client_machines} exceeds hosts in hosts.yml ({len(client_machines)})", file=sys.stderr)
        sys.exit(1)
    active_machines = client_machines[:n_client_machines]

    for i in range(n_clients):
        nid = cli_base + i
        machine = active_machines[i % n_client_machines]
        ip = machine["machine_ip"]
        client_entries.append(
            f'    {{ node_id = {nid}, ip = "{ip}", port = 10000, '
            f'hostname = "cli{nid}", node_type = "Client" }}'
        )

else:
    print(f"ERROR: unknown mode '{mode}'. Use 'local' or 'remote'.", file=sys.stderr)
    sys.exit(1)

body = (
    "bootstrap_nodes = [\n"
    + ",\n".join(replica_entries)
    + "\n]\n\n"
    "[own_node]\n"
    "port = 10000\n"
)

with open("$OUT", "w") as f:
    f.write(body)
print(f"Written $OUT ({n_replicas} replicas, {n_clients} clients in network map)")
PYEOF
