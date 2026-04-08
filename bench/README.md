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
6. [Adding a New Project](#adding-a-new-project)
7. [PKI / ca-root Generation](#pki--ca-root-generation)
8. [Generated Output](#generated-output)
9. [Variable Reference](#variable-reference)

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

# Force-regenerate TLS/signing certificates
make microbenchmarks-async regen-ca-root
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
| `regen-ca-root` | Delete and regenerate PKI certificates |
| `clean` | Remove all generated output (`bench/generated/`) |
| `help` | Print target summary and current variable values |

Any `bench.env` variable can be overridden on the command line:

```bash
make microbenchmarks-async local N_REPLICAS=7 N_CLIENTS=10 RUST_LOG=DEBUG
```

---

## Deployment Modes

### `local` — Docker Compose on this machine

Builds a Docker image from the shared `bench/Dockerfile` using the project's source tree, creates a Docker network (`atlas_network`), and starts one container per replica and one per client. Configs and ca-root are volume-mounted from `bench/generated/` — no files are baked into the image.

The network is torn down automatically when the Compose stack exits.

```bash
make microbenchmarks-async local
make microbenchmarks-async stop-local   # if you started it detached
make microbenchmarks-async logs-local
```

### `remote-docker` — Docker Compose on a remote cluster

Generates per-machine `docker-compose.yml` files and uses Ansible to push configs, ca-root, and compose files to each machine, then starts containers. Requires:
- A populated `hosts.yml` in the project's `bench/` directory
- A pre-built image at `DOCKER_IMAGE:DOCKER_VERSION` accessible from the cluster

```bash
make microbenchmarks-async remote-docker
make microbenchmarks-async stop-remote-docker
```

### `remote-bare` — Native binary on a remote cluster

Compiles the Rust binary with `RUSTFLAGS="-C target-cpu=native"`, then uses Ansible to push the binary, configs, ca-root, and identity env files to each machine and start processes. No Docker required on remote hosts.

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
    ├── scripts/
    │   ├── gen-ca-root.sh          generate PKI certificates into generated/ca-root/
    │   ├── gen-nodes-toml.sh       generate generated/nodes.toml
    │   ├── gen-local-compose.sh    generate generated/local-compose.yml
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
| `N_CLIENTS` | `1` | Total client processes (distributed round-robin across machines) |
| `CLIENTS_TO_RUN` | `5` | Logical clients per client process |
| `CONCURRENT_RQS` | `200` | Max in-flight requests per logical client |
| `OPS_NUMBER` | `1000000` | Total operations before the client exits |
| `REQUEST_SLEEP_MILLIS` | `0` | Sleep between requests (ms, 0 = no sleep) |
| `REQUEST_SIZE` | `0` | Request payload size in bytes |
| `REPLY_SIZE` | `0` | Reply payload size in bytes |
| `STATE_SIZE` | `0` | State snapshot size in bytes |
| `CA_ROOT_FORMAT` | `flattened` | PKI layout: `flattened` or `folder` (see [PKI section](#pki--ca-root-generation)) |
| `LOCAL_INFLUXDB` | `0` | Set to `1` to spin up a local InfluxDB 1.8 container |
| `RUST_LOG` | `INFO` | Rust log filter passed to all containers/processes |
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
      machine_ip: 192.168.1.20  # no node_id — assigned automatically from N_CLIENTS
```

- **Replicas**: one entry per machine, `node_id` must be unique and sequential from 0.
- **Clients**: one entry per client machine. `N_CLIENTS` processes are distributed round-robin across `N_CLIENT_MACHINES` of the listed hosts (in order). `node_id` is not specified — it is computed as `1000 + i`.

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

If you change `N_REPLICAS` or `N_CLIENTS`, regenerate:

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
| `generated/logs/` | runtime | Log output from replicas and clients |
| `generated/<BINARY_NAME>` | `build-binary` | Compiled Rust binary for remote-bare |
