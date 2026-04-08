#!/usr/bin/env bash
# gen-remote-envs.sh
#
# Generates per-node identity env files and per-machine client start scripts
# in generated/envs/, for use with remote-bare deployment.
#
# Reads from environment (exported by shared bench Makefile):
#   BENCH_DIR          - absolute path to project bench dir (for hosts.yml)
#   GENERATED          - absolute path to shared generated/ dir

set -euo pipefail

: "${BENCH_DIR:?BENCH_DIR not set}"
: "${GENERATED:?GENERATED not set}"
: "${N_CLIENTS:?N_CLIENTS not set}"
: "${N_CLIENT_MACHINES:?N_CLIENT_MACHINES not set}"

HOSTS_YML="$BENCH_DIR/hosts.yml"
OUT_DIR="$GENERATED/envs"

mkdir -p "$OUT_DIR"

python3 - <<PYEOF
import yaml, sys, os, stat

n_clients = int("$N_CLIENTS")
n_client_machines = int("$N_CLIENT_MACHINES")
out_dir = "$OUT_DIR"
cli_base = 1000

with open("$HOSTS_YML") as f:
    inv = yaml.safe_load(f)

# ── Replica env files ─────────────────────────────────────────────────────────────
for hostname, vals in inv["replicas"]["hosts"].items():
    nid = vals["node_id"]
    ip = vals["machine_ip"]
    content = (
        f"export OWN_NODE__NODE_ID={nid}\n"
        f"export OWN_NODE__IP={ip}\n"
        f"export OWN_NODE__HOSTNAME=srv{nid}\n"
        f"export OWN_NODE__NODE_TYPE=Replica\n"
    )
    path = os.path.join(out_dir, f".own_{nid}.env")
    with open(path, "w") as f:
        f.write(content)
    print(f"  {path}")

# ── Client env files + per-machine start scripts ───────────────────────────────────
client_machines = list(inv["clients"]["hosts"].items())
if n_client_machines > len(client_machines):
    print(f"ERROR: N_CLIENT_MACHINES={n_client_machines} exceeds client hosts ({len(client_machines)})", file=sys.stderr)
    sys.exit(1)
active_machines = client_machines[:n_client_machines]

machine_clients = {hostname: [] for hostname, _ in active_machines}
for i in range(n_clients):
    nid = cli_base + i
    hostname, vals = active_machines[i % n_client_machines]
    machine_ip = vals["machine_ip"]
    machine_clients[hostname].append((i, nid, machine_ip))

for hostname, clients in machine_clients.items():
    for idx, nid, ip in clients:
        content = (
            f"export OWN_NODE__NODE_ID={nid}\n"
            f"export OWN_NODE__IP={ip}\n"
            f"export OWN_NODE__HOSTNAME=cli{nid}\n"
            f"export OWN_NODE__NODE_TYPE=Client\n"
            f"export CLIENT=1\n"
        )
        path = os.path.join(out_dir, f".own_{nid}.env")
        with open(path, "w") as f:
            f.write(content)
        print(f"  {path}")

    script_lines = ["#!/usr/bin/env bash", "set -euo pipefail", ""]
    for idx, nid, ip in clients:
        script_lines.append(f'nohup ./run.sh {nid} > logs/client-{nid}.log 2>&1 &')
    script_lines += ["", "echo 'Waiting for all client processes to finish...'", "wait", "echo 'All clients done.'", ""]
    script = "\n".join(script_lines)

    script_path = os.path.join(out_dir, f"start-clients-{hostname}.sh")
    with open(script_path, "w") as f:
        f.write(script)
    os.chmod(script_path, os.stat(script_path).st_mode | stat.S_IXUSR | stat.S_IXGRP)
    print(f"  {script_path} ({len(clients)} client(s))")

print(f"\nGenerated env files and start scripts in {out_dir}/")
PYEOF
