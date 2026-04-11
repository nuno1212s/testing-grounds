#!/usr/bin/env bash
# gen-remote-envs.sh
#
# Generates per-node identity env files and per-machine client start scripts
# in generated/envs/, for use with remote-bare deployment.
#
# Reads from environment (exported by shared bench Makefile):
#   BENCH_DIR          - absolute path to project bench dir (for hosts.yml)
#   GENERATED          - absolute path to shared generated/ dir
#   N_CLIENT_MACHINES  - number of client machines to use
#
# Each client machine gets exactly one identity env file and one start script
# that launches a single process. That process runs N_CLIENTS logical clients
# internally (configured via client_config.toml).

set -euo pipefail

: "${BENCH_DIR:?BENCH_DIR not set}"
: "${GENERATED:?GENERATED not set}"
: "${N_CLIENT_MACHINES:?N_CLIENT_MACHINES not set}"

HOSTS_YML="$BENCH_DIR/hosts.yml"
OUT_DIR="$GENERATED/envs"

mkdir -p "$OUT_DIR"

python3 - <<PYEOF
import yaml, sys, os, stat

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

for i, (hostname, vals) in enumerate(active_machines):
    nid = cli_base + i
    ip = vals["machine_ip"]

    # Identity env file for this machine's single client process
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

    # Start script: one process per machine (N_CLIENTS logical clients from config)
    script_lines = [
        "#!/usr/bin/env bash",
        "set -euo pipefail",
        "",
        f"nohup ./run.sh {nid} > logs/client-{nid}.log 2>&1 &",
        "",
        "echo 'Waiting for client process to finish...'",
        "wait",
        "echo 'Client done.'",
        "",
    ]
    script = "\n".join(script_lines)

    script_path = os.path.join(out_dir, f"start-clients-{hostname}.sh")
    with open(script_path, "w") as f:
        f.write(script)
    os.chmod(script_path, os.stat(script_path).st_mode | stat.S_IXUSR | stat.S_IXGRP)
    print(f"  {script_path} (1 process, N_CLIENTS logical clients from config)")

print(f"\nGenerated env files and start scripts in {out_dir}/")
PYEOF
