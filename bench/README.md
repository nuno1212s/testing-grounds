# Atlas Bench — Shared Benchmarking Infrastructure

This directory is the single shared benchmarking layer for all projects in `testing-grounds/`. It provides a unified way to build, deploy, and run any Atlas project locally (Docker Compose) or on a remote cluster (Docker or bare-metal binary), all driven from the top-level `testing-grounds/Makefile`.

---

## Table of Contents

1. [Quick Start](#quick-start)
2. [Invocation](#invocation)
3. [Deployment Modes](#deployment-modes)
4. [Directory Layout](#directory-layout)
5. [Configuration](#configuration)
   - [Global bench.env](#global-benchenv)
   - [Per-project bench.env](#per-project-benchenv)
   - [hosts.yml](#hostsyml)
   - [Compile-time variants (EXECUTOR_VARIANT)](#compile-time-variants-executor_variant)
6. [WAN Emulation](#wan-emulation)
7. [Metrics and Grafana](#metrics-and-grafana)
8. [Adding a New Project](#adding-a-new-project)
9. [PKI / ca-root Generation](#pki--ca-root-generation)
10. [Generated Output](#generated-output)

---

## Quick Start

```bash
# From testing-grounds/

# Run 4 replicas + 1 client locally in Docker
make microbenchmarks-async local

# Override cluster size on the fly
make microbenchmarks-async local N_REPLICAS=3 N_CLIENTS=4

# Deploy to a remote cluster using Docker
make app-scaling-tests remote-docker

# Deploy as native binaries (no Docker)
make microbenchmarks remote-bare

# Stop a running local deployment
make microbenchmarks-async stop-local

# Regenerate configs (e.g. after changing N_REPLICAS in bench.env)
make microbenchmarks-async gen-configs

# Emulate a WAN locally: check the kernel once, preview the topology, then run
make microbenchmarks-async wan-check
make microbenchmarks-async wan-plan WAN_PROFILE=wan-global-3region
make microbenchmarks-async local WAN_ENABLED=1 WAN_PROFILE=wan-global-3region

# Force-regenerate TLS/signing certificates
make microbenchmarks-async regen-ca-root

# The metrics stack (InfluxDB + Grafana) starts automatically before any run, so
# there is normally nothing to do. These are for driving it by hand — no project
# name needed, and it can be started before or after a run.
make metrics
make stop-metrics
```

---

## Invocation

All commands are run from `testing-grounds/` using a two-argument form:

```
make <project> <target> [VAR=value ...]
```

**Available projects:**

| Project name | Source directory |
|---|---|
| `microbenchmarks-async` | `microbenchmarks-async/bench` |
| `app-scaling-tests` | `app-scaling-tests/bench` |
| `preemptive_execution` | `preemptive_execution/bench` |
| `microbenchmarks` | `microbenchmarks/bench` |
| `microbenchmark-hotstuff` | `hot_stuff/microbenchmark-hotstuff/bench` |
| `microbenchmark-chainedhotstuff` | `hot_stuff/microbenchmark-chainedhotstuff/bench` |

**Available targets:**

| Target | Description |
|---|---|
| `local` | Build Docker image and run replicas + clients locally |
| `stop-local` | Stop and remove local containers and network |
| `logs-local` | Follow logs from all local containers |
| `remote-docker` | Deploy pre-built Docker image to SSH cluster via Ansible |
| `stop-remote-docker` | Stop remote Docker deployment |
| `remote-bare` | Compile binary and deploy as a bare process via Ansible |
| `stop-remote-bare` | Kill remote bare processes |
| `build-binary` | Compile the Rust binary only (used by `remote-bare`) |
| `gen-configs` | Generate config files and ca-root into `generated/` |
| `gen-ca-root` | Generate PKI certificates only (skips if already present) |
| `metrics` | Start InfluxDB + Grafana on `atlas_network` and wait for the database (no project needed) |
| `stop-metrics` | Stop the metrics stack, keeping its data volumes |
| `logs-metrics` | Follow InfluxDB + Grafana logs |
| `clean-metrics` | Stop the metrics stack and delete its data volumes (**including every measurement**) |
| `wan-check` | Verify this host's kernel supports netem (run once, before first WAN use) |
| `wan-plan` | Compile the WAN profile and print the latency matrix (no containers) |
| `wan-show` | Show live `tc` stats inside the running containers |
| `wan-apply` | Re-apply the WAN profile to an already-running cluster |
| `regen-ca-root` | Delete and regenerate PKI certificates |
| `clean` | Remove all generated output (`bench/generated/`) |
| `help` | Print target summary and current variable values |

`clean`, `help` and the four `metrics` targets act on the shared bench dir only and so
take no project name — `make metrics` on its own is the normal form. `grafana`,
`stop-grafana`, `logs-grafana` and `clean-grafana` remain as aliases from when the
stack was Grafana alone.

Any `bench.env` variable can be overridden on the command line:

```bash
make microbenchmarks-async local N_REPLICAS=7 N_CLIENTS=10 RUST_LOG=DEBUG
```

---

## Deployment Modes

### `local` — Docker Compose on this machine

Builds a Docker image from the shared `bench/Dockerfile` using the project's source tree, creates a Docker network (`atlas_network`), and starts one container per replica and one per client. Configs and ca-root are volume-mounted from `bench/generated/` — no files are baked into the image.

If the project selects a compile-time variant (`EXECUTOR_VARIANT`, see [Compile-time variants](#compile-time-variants-executor_variant)), that selection is compiled into this image and encoded in its tag.

The network is torn down automatically when the Compose stack exits.

Local mode can additionally emulate a WAN — per-link latency, jitter, loss and
bandwidth caps between containers. See [WAN Emulation](#wan-emulation).

```bash
make microbenchmarks-async local
make microbenchmarks-async stop-local   # if you started it detached
make microbenchmarks-async logs-local
```

### `remote-docker` — Docker Compose on a remote cluster

Generates per-machine `docker-compose.yml` files and uses Ansible to push configs, ca-root, and compose files to each machine, then starts containers. Requires:
- A populated `hosts.yml` in the project's `bench/` directory
- A pre-built image at `DOCKER_IMAGE:DOCKER_VERSION` accessible from the cluster

This mode **builds nothing** — each machine pulls the registry image. A compile-time variant therefore cannot be chosen at deploy time; it has to be baked in when that image is built and pushed. See [Compile-time variants](#compile-time-variants-executor_variant).

```bash
make microbenchmarks-async remote-docker
make microbenchmarks-async stop-remote-docker
```

### `remote-bare` — Native binary on a remote cluster

Compiles the Rust binary with `RUSTFLAGS="-C target-cpu=native"`, then uses Ansible to push the binary, configs, ca-root, and identity env files to each machine and start processes. No Docker required on remote hosts. Honours `EXECUTOR_VARIANT`.

```bash
make microbenchmarks-async remote-bare
make microbenchmarks-async stop-remote-bare
```

For `remote-bare` to work, the local machine and the remote hosts must have compatible CPU architectures.

---

## Directory Layout

```
testing-grounds/
├── Makefile                        top-level router (the entry point)
└── bench/
    ├── README.md                   this file
    ├── bench.env                   global defaults for all projects
    ├── Makefile                    shared parameterized Makefile (not called directly)
    ├── Dockerfile                  single shared Dockerfile, parameterized by APP_NAME
    ├── config-base/
    │   ├── common/                 config templates shared by all nodes
    │   │   ├── febft.toml
    │   │   ├── influx_db.toml
    │   │   └── ...
    │   ├── replica/                replica-only config templates
    │   │   ├── benchmark_config.toml
    │   │   └── ...
    │   └── client/                 client-only config templates
    │       ├── benchmark_config.toml
    │       ├── client_config.toml
    │       └── ...
    ├── wan-profiles/               WAN topology profiles (see WAN Emulation)
    │   ├── none.yml                zero-delay control profile
    │   ├── lan.yml
    │   ├── wan-eu-us.yml
    │   ├── wan-global-3region.yml
    │   ├── wan-global-5region.yml
    │   └── lossy-wan.yml
    ├── grafana/                    Grafana stack (its own Compose project)
    │   ├── README.md               how it finds InfluxDB, how to read the panels
    │   ├── docker-compose.yml      joins atlas_network; publishes :3000
    │   ├── provisioning/
    │   │   └── dashboards/         dashboard provider (datasource is generated)
    │   ├── build-dashboards.py     generates dashboards/ from the Rust metric registries
    │   └── dashboards/
    │       ├── atlas-cluster-overview.json
    │       └── suite-<project>.json     one per test suite
    ├── scripts/
    │   ├── gen-ca-root.sh          generate PKI certificates into generated/ca-root/
    │   ├── gen-nodes-toml.sh       generate generated/nodes.toml
    │   ├── gen-local-compose.sh    generate generated/local-compose.yml
    │   ├── gen-grafana.sh          resolve the InfluxDB target, write the server env,
    │   │                           Grafana's env and the provisioned datasource
    │   ├── metrics-up.sh           start InfluxDB + Grafana, wait for the database
    │   ├── gen-wan.py              compile a WAN profile into per-node tc scripts
    │   ├── wan-entrypoint.sh       baked into the final-wan image; applies tc, execs server
    │   ├── gen-remote-compose.sh   generate generated/per-machine/*-compose.yml
    │   └── gen-remote-envs.sh      generate generated/envs/ (bare deployment identities)
    ├── playbooks/
    │   ├── remote-docker-deploy.yml
    │   ├── remote-docker-stop.yml
    │   ├── remote-bare-deploy.yml
    │   └── remote-bare-stop.yml
    └── generated/                  all generated output (gitignored)
        ├── ca-root/
        ├── config-replicas/
        ├── config-clients/
        ├── nodes.toml
        ├── local-compose.yml
        ├── grafana/                influxdb.env + grafana.env + provisioned datasource
        ├── per-machine/            per-machine compose files (remote-docker)
        ├── envs/                   per-node identity envs + start scripts (remote-bare)
        ├── logs/
        └── <BINARY_NAME>           compiled binary (remote-bare)
```

Per-project bench directories contain only what differs from the global defaults:

```
<project>/bench/
├── bench.env     project identity variables + any workload overrides
└── hosts.yml     cluster inventory for remote deployments
```

---

## Configuration

### Global bench.env

`bench/bench.env` sets defaults for all projects. Values here are loaded first and can be overridden by any project's `bench.env`.

| Variable | Default | Description |
|---|---|---|
| `N_REPLICAS` | `4` | Number of replica processes |
| `N_CLIENT_MACHINES` | `1` | Number of machines running client processes |
| `N_CLIENTS` | `5` | Logical clients per client machine (total = `N_CLIENTS × N_CLIENT_MACHINES`). Each machine runs one process that spawns this many clients internally. |
| `CONCURRENT_RQS` | `200` | Max in-flight requests per logical client |
| `OPS_NUMBER` | `1000000` | Total operations before the client exits |
| `REQUEST_SLEEP_MILLIS` | `0` | Sleep between requests (ms, 0 = no sleep) |
| `REQUEST_SIZE` | `0` | Request payload size in bytes |
| `REPLY_SIZE` | `0` | Reply payload size in bytes |
| `STATE_SIZE` | `0` | State snapshot size in bytes |
| `CA_ROOT_FORMAT` | `flattened` | PKI layout: `flattened` or `folder` (see [PKI section](#pki--ca-root-generation)) |
| `LOCAL_INFLUXDB` | `0` | Set to `1` to spin up a local InfluxDB 1.8 container |
| `INFLUX_EXTRA` | unset | Run name; becomes the `extra` tag on every metric point, and Grafana's "Run" filter. Left unset (not empty) so the tag keeps its `None` default |
| `GRAFANA_IMAGE` | `grafana/grafana:11.4.0` | Grafana image for `make grafana` |
| `GRAFANA_PORT` | `3000` | Host port Grafana is published on |
| `GRAFANA_BIND` | `127.0.0.1` | Host interface for that port. Loopback because anonymous access is on |
| `GRAFANA_ANONYMOUS` | `true` | Anonymous (Admin-role) access, so there is nothing to log into locally |
| `GRAFANA_ADMIN_USER` / `GRAFANA_ADMIN_PASSWORD` | `admin` / `admin` | Grafana admin login |
| `GRAFANA_INFLUX_URL` / `_DB` / `_USER` / `_PASSWORD` | empty | Override the resolved InfluxDB target; empty means derive it from `influx_db.toml` + `LOCAL_INFLUXDB` |
| `RUST_LOG` | `INFO` | Rust log filter passed to all containers/processes |
| `WAN_ENABLED` | `0` | `1` enables tc/netem WAN emulation (see [WAN Emulation](#wan-emulation)) |
| `WAN_PROFILE` | `wan-global-3region` | Profile basename in `bench/wan-profiles/` |
| `WAN_SUBNET` | `10.90.0.0/24` | Subnet for static container IPs (WAN mode only) |
| `WAN_IFACE` | `eth0` | Interface shaped inside each container |
| `WAN_SHAPE_CLIENTS` | `1` | `0` shapes replica↔replica links only |
| `WAN_TIMEOUT_SCALE` | `1` | Multiplies `timeout_duration` in the generated configs |
| `ANSIBLE_USER` | `nneto` | SSH user for Ansible remote deployments |
| `REMOTE_WORKDIR` | `/home/nneto/atlas-bench` | Working directory on remote hosts |

### Per-project bench.env

Each project's `bench.env` must define four identity variables and may override any global default.

**Required identity variables:**

| Variable | Description |
|---|---|
| `PROJECT_NAME` | Human-readable label for logs |
| `IMAGE_NAME` | Docker image tag for `make local` (local builds) |
| `BINARY_NAME` | Cargo `[[bin]]` name; must match `target/release/<name>` |
| `RUST_SRC_RELPATH` | Path from `<project>/bench/` to the Cargo workspace/package root |

**Optional remote-docker variables:**

| Variable | Description |
|---|---|
| `DOCKER_IMAGE` | Registry image name for `remote-docker` mode |
| `DOCKER_VERSION` | Image tag/version for `remote-docker` mode |

**Variables computed automatically — do not set in bench.env:**

| Variable | How it is derived |
|---|---|
| `APP_SOURCE_DIR` | Derived from `RUST_SRC_RELPATH` relative to the Atlas repo root (used as a Docker build arg) |
| `CARGO_FEATURES` | Copied from `EXECUTOR_VARIANT`; empty for projects that set none. Reaches `build-binary` as cargo flags and the Docker build as a build arg |
| `IMAGE_NAME_EFFECTIVE` | `IMAGE_NAME`, plus `-<variant>` when `CARGO_FEATURES` is set, plus `-wan` in WAN mode |
| `DOCKERFILE_ABS` | Always `bench/Dockerfile` |
| `BUILD_CTX_ABS` | Always the Atlas repo root (two levels above `bench/`) |

Example per-project `bench.env`:

```bash
# ── Required identity ──────────────────────────────────────────────────────
PROJECT_NAME=my-project
IMAGE_NAME=my-project
BINARY_NAME=my-project-bin
RUST_SRC_RELPATH=../rust

# ── Remote-docker image ────────────────────────────────────────────────────
DOCKER_IMAGE=myregistry/my-project
DOCKER_VERSION=v1.0.0

# ── Workload overrides (optional) ─────────────────────────────────────────
# N_REPLICAS=3

# ── App-specific runtime vars ─────────────────────────────────────────────
# MY_FEATURE_FLAG=enabled
```

Any extra variables added to a project's `bench.env` are automatically injected into containers (via `env_file:`) and exported to bare processes (via the shared `.env` file).

### hosts.yml

Required for `remote-docker` and `remote-bare` modes. Lives in `<project>/bench/hosts.yml`.

```yaml
replicas:
  hosts:
    hostname-of-machine:
      ansible_host: hostname-or-ip
      ansible_user: nneto
      node_id: 0          # must match the replica's node ID (0-indexed)
      machine_ip: 192.168.1.10   # IP used for inter-node communication

clients:
  hosts:
    client-machine-1:
      ansible_host: hostname-or-ip
      ansible_user: nneto
      machine_ip: 192.168.1.20  # no node_id — assigned automatically (cli_base + index)
```

- **Replicas**: one entry per machine, `node_id` must be unique and sequential from 0.
- **Clients**: one entry per client machine. `N_CLIENT_MACHINES` of the listed hosts are used (in order). Each machine runs one process with `N_CLIENTS` logical clients. `node_id` is not specified — it is computed as `1000 + i`.

### Compile-time variants (`EXECUTOR_VARIANT`)

Some binaries pick a variant of themselves at compile time, because the choice is a Rust
type alias and cannot be a runtime switch. `crud_perf` is the one that does today: its
`EXECUTOR_VARIANT` names one of four mutually exclusive Cargo features (`baseline`,
`dual_state`, `crud_single`, `crud_scalable`), and the chosen name is what the binary
stamps as the InfluxDB `extra` tag, so runs are self-identifying at analysis time.

The bench system carries this generically, under the name `CARGO_FEATURES`. Set
`EXECUTOR_VARIANT` in the project's `bench.env` or override it per run:

```bash
make crud_perf local        EXECUTOR_VARIANT=baseline
make crud_perf build-binary EXECUTOR_VARIANT=baseline
```

| Mode | Honours `EXECUTOR_VARIANT`? | How |
|---|---|---|
| `local` | yes | `--build-arg CARGO_FEATURES=<variant>` on the image build, mirrored into `build.args` of `generated/local-compose.yml` |
| `build-binary` / `remote-bare` | yes | `--no-default-features --features <variant>` on the cargo invocation |
| `remote-docker` | **no** | Builds nothing; every machine pulls `DOCKER_IMAGE:DOCKER_VERSION`. The target prints a warning naming the variant it is ignoring |

Empty (the normal case — no other project sets it) means both paths run exactly the
cargo/`docker build` command they ran before this arg existed.

**The image tag varies with the variant.** `IMAGE_NAME_EFFECTIVE` becomes
`<IMAGE_NAME>-<variant>` (`crud-perf-baseline`, `crud-perf-crud-single`; underscores and
commas folded to dashes to stay a legal tag), and `-wan` is still appended on top in WAN
mode. Without this a cached image from the previous variant would be reused and the run
would silently benchmark the wrong executor.

For `remote-docker`, build and push a per-variant tag and select it with `DOCKER_VERSION`:

```bash
docker build --build-arg APP_NAME=crud_perf_exec \
             --build-arg APP_SOURCE_DIR=testing-grounds/crud_perf/crud_perf_exec \
             --build-arg CARGO_FEATURES=baseline \
             -f testing-grounds/bench/Dockerfile \
             -t nukino/atlas-crud-perf:baseline .          # from the Atlas repo root
docker push nukino/atlas-crud-perf:baseline
make crud_perf remote-docker DOCKER_VERSION=baseline
```

Confirm what actually ran rather than what you asked for: each replica logs
`executor_variant=<name>` at startup, and the same name lands in InfluxDB —

```bash
docker exec atlas-influxdb influx -database atlas \
    -execute 'SHOW TAG VALUES WITH KEY = "extra"'
```

---

## WAN Emulation

`make <project> local` normally runs every node on one Docker bridge, where each link
is a ~0.1 ms loopback. WAN emulation injects realistic per-link **latency, jitter,
packet loss and bandwidth caps** between containers using `tc`/`netem`, so
geographically distributed deployments can be studied locally.

It is **off by default and inert when off**: with `WAN_ENABLED=0` the generated
`local-compose.yml` is byte-identical to what it was before this feature existed, and
containers run the same distroless image as always.

### Before first use: `wan-check`

netem is a kernel module (`sch_netem`). Some container VMs ship without it, and
nothing else in the stack can substitute — netem is the only qdisc that adds delay.
Check once:

```bash
make crud_perf wan-check
```

If it fails on **podman on macOS**, the podman machine runs Fedora CoreOS, whose base
image omits `sch_netem`. Add it once:

```bash
podman machine ssh 'sudo rpm-ostree install kernel-modules-extra'
podman machine stop && podman machine start
make crud_perf wan-check
```

The layered package survives VM reboots, but not `podman machine rm`. Undo it with
`podman machine ssh 'sudo rpm-ostree rollback'`.

### Running

```bash
# See the topology a profile produces — no containers, fast; this is the authoring loop
make crud_perf wan-plan WAN_PROFILE=wan-global-3region

# Run the benchmark under it
make crud_perf local WAN_ENABLED=1 WAN_PROFILE=wan-global-3region

# While it runs, from another shell:
make crud_perf wan-show                       # live tc counters per node
podman exec replica-0 ping -c 5 replica-2     # confirm the RTT is what you asked for

# Change conditions without restarting the cluster
make crud_perf wan-apply WAN_ENABLED=1 WAN_PROFILE=lossy-wan
```

### Shipped profiles

| Profile | What it models |
|---|---|
| `none` | Zero delay, zero loss, no caps. The **control**: a run with `WAN_PROFILE=none` must match a `WAN_ENABLED=0` run within noise. |
| `lan` | A single well-provisioned datacenter (0.4 ms RTT). |
| `wan-eu-us` | One transatlantic link, 2+2 replicas, so no region holds a quorum. |
| `wan-global-3region` | Three continents with AWS-like RTTs, plus one deliberately asymmetric link. The default. |
| `wan-global-5region` | Five regions, one replica each. Run with `N_REPLICAS=5`. |
| `lossy-wan` | High jitter, real loss, a narrow pipe. For liveness/timeout testing, not throughput. |

### Writing a profile

Profiles live in `bench/wan-profiles/<name>.yml`. Nodes are assigned to regions, links
are declared between regions, and per-node-pair `overrides` handle anything the region
model cannot express — including asymmetric links.

```yaml
defaults:
  rtt_ms: 0
  jitter_ms: 0
  loss_pct: 0
  rate: 10gbit          # HTB ceiling; 10gbit means "no meaningful cap"
  distribution: normal
  netem_limit: auto     # backlog in packets; auto = from bandwidth-delay product

placement:              # node -> region; globs allowed, first match wins
  "replica-0": eu-west
  "replica-1": eu-west
  "replica-2": us-east
  "replica-3": ap-south
  "client-*":  eu-west

intra_region: { rtt_ms: 1, jitter_ms: 0.2 }

links:
  - { a: eu-west, b: us-east,  rtt_ms: 78,  jitter_ms: 4, loss_pct: 0.01 }
  - { a: eu-west, b: ap-south, rtt_ms: 125, jitter_ms: 8, loss_pct: 0.05 }
  - { a: us-east, b: ap-south, rtt_ms: 198, jitter_ms: 10, loss_pct: 0.05 }

overrides:
  # Directional: pins one direction only. owd_ms is taken verbatim, unhalved.
  - { from: "replica-2", to: "replica-3", owd_ms: 200 }
  # Symmetric node-pair override:
  - { between: ["replica-0", "client-0"], rtt_ms: 20 }

schedule: []            # reserved for timed events; parsed and validated, not yet applied
```

Every node in the run must match a `placement` entry — `wan-plan` fails loudly listing
any it cannot place, which is what catches "I bumped `N_REPLICAS` but not the profile".

**Round-trip vs one-way.** Shaping is applied on egress at *both* ends, so profile
values describing a round trip are split across the two directions using the exact
formulas, not approximations:

| Profile key | Per-direction value |
|---|---|
| `rtt_ms` | `rtt_ms / 2` (delays add) |
| `jitter_ms` | `jitter_ms / sqrt(2)` (independent variances add) |
| `loss_pct` | `1 - sqrt(1 - loss_pct/100)` (round-trip success is `(1-p)^2`) |

Use `owd_ms`, `owd_jitter_ms` and `owd_loss_pct` to set a direction verbatim with no
splitting. `wan-plan` prints the resulting per-direction matrix, so the numbers that
will actually be installed are always visible before a run.

**Jitter causes reordering.** `netem delay X Y distribution normal` lets packets
overtake one another. That is realistic, but for strictly reproducible, in-order runs
set `jitter_ms: 0`. `wan-plan` warns when a profile uses jitter.

### Protocol timeouts

The timeouts in `config-base/` assume a LAN. Under a high-delay or lossy profile they
can fire spuriously and trigger view changes that are an artifact of the emulation
rather than a property of the protocol. `WAN_TIMEOUT_SCALE` multiplies
`timeout_duration` in the **generated** configs (`febft.toml`, `log_transfer.toml`,
`state_transfer.toml`, `view_transfer.toml`):

```bash
make crud_perf local WAN_ENABLED=1 WAN_PROFILE=lossy-wan WAN_TIMEOUT_SCALE=4
```

The scaled value is always derived from the pristine `config-base` template, so this is
idempotent and `config-base` itself is never modified.

### How it works

| Piece | Role |
|---|---|
| `bench/Dockerfile` stage `final-wan` | `debian:bullseye-slim` + `iproute2`. Used **only** when `WAN_ENABLED=1`, tagged `<IMAGE_NAME>-wan` so it never collides with the normal image. The default `final` stage stays distroless and stays last in the file, which is what keeps the off-path unchanged. |
| `scripts/wan-entrypoint.sh` | Baked into that image. Applies `/wan/$WAN_NODE.sh`, then `exec`s the server — so shaping is in place *before* the first packet, and a failure to shape aborts the container rather than silently producing an unshaped run. |
| `scripts/gen-wan.py` | Compiles the profile into `generated/wan/`: one `tc` script per node, `matrix.txt`, and `profile.resolved.yml` (the fully expanded profile, worth keeping with the results). |
| Static IPs | WAN mode assigns addresses from `WAN_SUBNET` (`replica-i` → `.10+i`, `client-i` → `.100+i`) so destination-IP filters survive the `restart: on-failure` these services already use. Node names still resolve normally, so `nodes.toml` is unchanged. |
| `tc` structure | One HTB class per peer, each carrying its own `netem`, selected by a `u32` filter on destination IP. Unmatched traffic falls into an unshaped default class. |

The `/wan` directory is bind-mounted, not baked in, so `wan-apply` can change
conditions on a running cluster without a rebuild.

### Limitations

- All `N_CLIENTS` logical clients in one container share one profile. To place clients
  in different regions raise `N_CLIENT_MACHINES` (one container per region), and
  remember `make <project> regen-ca-root` after changing cluster sizing.
- Traffic arriving through published host ports (`10000+i`) is **not** shaped; only
  container-to-container traffic is.
- The `final-wan` image runs as root with `CAP_NET_ADMIN`. Fine for a local harness;
  don't push it to a registry for cluster use.
- Shaping is egress-only, so each node controls only its own outbound half of a link.
- WAN emulation covers **local mode only**. `remote-docker` and `remote-bare` are
  untouched — those already run on a real network.
- Rootless podman may refuse `CAP_NET_ADMIN` inside the container netns; `wan-check`
  is what surfaces that.

---

## Metrics and Grafana

Every Atlas node runs a metrics thread that collects and resets its metrics once a
second and writes them to InfluxDB. Where they go is set by
`config-base/common/influx_db.toml` — one file, read by replicas and clients alike.

InfluxDB and Grafana are one Compose project, `bench/grafana/docker-compose.yml`, and
**every target that starts a run brings it up first and blocks until the database
answers**. In the normal case there is nothing to do:

```bash
make crud_perf local INFLUX_EXTRA=baseline-4r   # tag the run so you can pick it out later
# → metrics stack up, InfluxDB ready, then the cluster starts
#   Grafana on http://127.0.0.1:3000, folder 'Atlas'
```

`LOCAL_INFLUXDB=1` (the default) means the stack runs the database itself and the
*generated* configs are rewritten to reach it at `http://influxdb:8086`; the
checked-in `influx_db.toml` keeps naming the external instance either way. With
`LOCAL_INFLUXDB=0` the `influxdb` service is filtered out by its Compose profile and
everything — replicas, clients, Grafana — talks to the instance that file names.

### Why the wait matters

The gate is not politeness. Atlas' OS monitor thread writes every 250 ms through

```rust
rt::block_on(client.query(readings)).expect("Failed to write metrics to influxdb")
```

(`Atlas/Atlas-Metrics/src/metrics/os_mon.rs`), and the workspace's release profile sets
`panic = "abort"` — the profile both the bench Dockerfile and `make build-binary` use.
So a node that starts before InfluxDB is listening does not run without OS metrics; it
aborts within the first second. That is why `scripts/metrics-up.sh` polls until the
database answers `SHOW DATABASES` and refuses to start the run if it never does, and
why `METRICS_AUTOSTART=0` prints a warning rather than quietly skipping ahead.

### Driving the stack by hand

```bash
make metrics            # start it (or re-run: idempotent)
make logs-metrics
make stop-metrics       # keeps both data volumes
make clean-metrics      # drops them — every stored measurement goes with it
```

The metrics stack is **its own Compose project**, not part of the generated benchmark
stack, because it has to outlive the run it is showing — a database torn down with the
run is empty exactly when there is something to look at. The two stacks meet on the
`atlas_network` bridge, which both declare `external: true` and which the Makefile's
`ensure-network` target owns:

```
  make metrics ──────┐                               ┌─ make <project> local
  (or automatically, ▼                               ▼
   before a run)  ┌─────────────────────┐   ┌──────────────────────────────┐
                  │ atlas-influxdb :8086│◀──│ replica-0..n-1, client-0..m  │
                  │ atlas-grafana  :3000│   │      (write metrics)         │
                  └──────────┬──────────┘   └───────────────┬──────────────┘
                             └────────── atlas_network ─────┘
                     (created by ensure-network, removed
                      only when nothing is attached)
```

So the stack can start before or after a run, and a bench teardown leaves it running:
removing the network fails while the stack holds it, and the failure is ignored by
design.

`make metrics` (and the automatic start) first runs `scripts/gen-grafana.sh`, which
resolves the InfluxDB the replicas use — the in-network container when
`LOCAL_INFLUXDB=1`, otherwise the instance from `influx_db.toml` — and writes
`generated/grafana/`: the server's own env (database name and admin user), Grafana's
env, and the provisioned datasource. All three come from that one TOML, so the
writers, the database and Grafana cannot drift apart.

### Reaching it from a remote cluster

`remote-docker` and `remote-bare` start the stack too, but the nodes there read the
*unmodified* `influx_db.toml` — only `gen-local-compose.sh` rewrites the address — and
they cannot resolve the `influxdb` alias, which exists only on `atlas_network`. For a
remote cluster to write into this host's database, point `influx_db.toml` at this host
and publish the port beyond loopback:

```bash
make microbenchmarks-async remote-bare INFLUXDB_BIND=0.0.0.0 INFLUXDB_AUTH_ENABLED=true
```

The run targets print this reminder themselves.

Eight dashboards are provisioned into the **Atlas** folder: one per test suite, plus a
cross-suite *Cluster Overview* to start from.

| Dashboard | `make` target | Ordering protocol |
|---|---|---|
| Cluster Overview | — | any |
| microbenchmarks-async | `make microbenchmarks-async local` | febft PBFT |
| app-scaling-tests | `make app-scaling-tests local` | febft PBFT |
| preemptive_execution | `make preemptive_execution local` | febft PBFT |
| microbenchmarks (legacy) | `make microbenchmarks local` | febft PBFT |
| crud_perf | `make crud_perf local` | febft PBFT |
| microbenchmark-hotstuff | `make microbenchmark-hotstuff local` | HotStuff (four-phase) |
| microbenchmark-chainedhotstuff | `make microbenchmark-chainedhotstuff local` | Chained HotStuff |

Every suite dashboard has the same shape — headline stats, then that suite's **ordering
protocol** rows, then its **executor** rows, then a shared **Atlas baseline** (client,
request pre-processing, replica loop, communication, logging and transfer, host
resources). The baseline is deliberately identical everywhere, so two suites can be
compared panel-for-panel.

Panels are generated, not hand-written: `grafana/build-dashboards.py` parses the metric
registries out of the Rust sources and draws only what a given suite actually emits —
which depends both on the crates its binary registers and on its
`with_metric_level(..)`. Each dashboard's *About this suite* note names the metrics its
level filters out, so a blank panel is explained rather than mysterious. Regenerate
with:

```bash
python3 bench/grafana/build-dashboards.py
```

Both variables — **Node** (`host`) and **Run** (`extra`) — filter every panel. Filtering
by Run matters more than it looks: all suites share one database, and `PREPARE_LATENCY`
and `COMMIT_LATENCY` mean different things in febft and in HotStuff.

`correctness-testing` and IronDumbo have no dashboard: the first is an in-process
`cargo nextest` library that never initialises metrics, the second has no metrics
instrumentation at all. Neither writes to InfluxDB, so neither needs the metrics stack
running — `correctness-testing` is not a `make` run target in any case. See
[`grafana/README.md`](grafana/README.md) for the per-suite metric analysis, how to read
each metric kind, and two measurement quirks worth knowing before trusting a panel.

### With WAN emulation

`WAN_ENABLED=1` requires `atlas_network` to carry `WAN_SUBNET`, so if the network
already exists with a different subnet it must be recreated — which cannot happen
while the metrics stack is attached. `make stop-metrics` first; the Makefile says as
much if you forget. InfluxDB and Grafana both take dynamic addresses at the low end of
the subnet, clear of the static offsets (replicas `.10+i`, clients `.100+i`), and are
left unshaped: they match no `tc` filter and land in the default class, so collecting
metrics does not consume emulated WAN bandwidth.

---

## Adding a New Project

1. Create `<project>/bench/bench.env` with the four required identity variables.

2. Create `<project>/bench/hosts.yml` if you plan to use remote deployments (can be empty for local-only).

3. Register the project in `testing-grounds/Makefile`:
   ```makefile
   PROJECTS := ... my-new-project

   bench_dir_my-new-project := my-new-project/bench
   ```

4. Verify:
   ```bash
   make my-new-project gen-configs   # should populate bench/generated/
   make my-new-project local         # should build and start containers
   ```

---

## PKI / ca-root Generation

TLS certificates and Ed25519 signing keys are **generated automatically** on the first run of any target that depends on `gen-configs`. They are stored in `bench/generated/ca-root/` and reused for all subsequent runs.

If you change `N_REPLICAS` or `N_CLIENT_MACHINES`, regenerate:

```bash
make <project> regen-ca-root
```

### Formats

Two layouts are supported, controlled by `CA_ROOT_FORMAT` in `bench.env`:

**`flattened`** (default) — used with `FlattenedPathConstructor` from `atlas-default-configs`:
```
generated/ca-root/
  ca-root-cert             root CA certificate (PEM)
  ca-root-private          root CA key (PKCS8 DER, for ring)
  ca-root-private_pem      root CA key (PEM, for rustls)
  ca-root-public_pcks      root CA public key (DER)
  ca-root-0-cert           replica 0 certificate (PEM)
  ca-root-0-private        replica 0 key (PKCS8 DER, Ed25519, for ring::KeyPair::from_pkcs8)
  ca-root-0-private_pem    replica 0 key (PEM, for rustls TLS)
  ca-root-0-public         replica 0 public key (PEM)
  ca-root-0-public_pcks    replica 0 public key (DER)
  ca-root-1000-*           client 1000 (same set of files)
  ...
```

**`folder`** — used with `FolderPathConstructor` (matches the Atlas `keygen` tool output):
```
generated/ca-root/
  cert                     root CA certificate (PEM)
  private                  root CA key (PKCS8 DER)
  private_pem              root CA key (PEM)
  public_pcks              root CA public key (DER)
  0/                       replica 0
    cert
    private
    private_pem
    public
    public_pcks
  1000/                    client 1000
    ...
```

Set `CA_ROOT_FORMAT=folder` in a project's `bench.env` to use the folder layout.

All keys are **Ed25519**, matching what ring's `KeyPair::from_pkcs8` and rustls expect.

---

## Generated Output

Everything under `bench/generated/` is gitignored and recreated on demand. It is **shared across all projects** — running `gen-configs` for one project overwrites the previous run's output.

This is intentional: you work on one project at a time. If you need to compare two projects, run `clean` between them, or run them sequentially.

| Path | Created by | Contents |
|---|---|---|
| `generated/ca-root/` | `gen-ca-root` | TLS certs + Ed25519 signing keys |
| `generated/config-replicas/` | `gen-configs` | TOML config files for replicas |
| `generated/config-clients/` | `gen-configs` | TOML config files for clients |
| `generated/nodes.toml` | `gen-nodes-toml.sh` | Network topology (node IDs + IPs) |
| `generated/local-compose.yml` | `gen-local-compose.sh` | Docker Compose file for local mode |
| `generated/per-machine/` | `gen-remote-compose.sh` | Per-machine compose files for remote-docker |
| `generated/envs/` | `gen-remote-envs.sh` | Per-node identity env files + client start scripts for remote-bare |
| `generated/wan/` | `gen-wan.py` | Per-node `tc` scripts, `matrix.txt`, `profile.resolved.yml` |
| `generated/grafana/` | `gen-grafana.sh` | `influxdb.env` (server: db name + admin user), `grafana.env` (resolved InfluxDB target) + provisioned datasource |
| `generated/logs/` | runtime | Log output from replicas and clients |
| `generated/<BINARY_NAME>` | `build-binary` | Compiled Rust binary for remote-bare |
