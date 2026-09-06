# Atlas Testing Grounds

A collection of benchmarks, experiments, and integration tests for the [Atlas framework](../Atlas) — a modular Rust framework for building Byzantine Fault Tolerant (BFT) and Crash Fault Tolerant (CFT) consensus protocols.

---

## About Atlas

Atlas provides high-performance, modular building blocks for implementing consensus protocols (PBFT-based, probabilistic, blockchain-focused, or State Machine Replication). It is written in Rust for memory safety, near-native performance, and safe concurrent programming without garbage collection overhead.

Key modules in Atlas:

| Module | Purpose |
|---|---|
| `atlas-common` | Core utilities and abstractions |
| `atlas-core` | BFT/CFT protocol components |
| `atlas-communication` | Network communication layer |
| `atlas-smr-application` | Application abstractions for SMR |
| `atlas-smr-replica` | Replica implementation |
| `atlas-client` | Client implementation |
| `atlas-persistent-log` | Log persistence |
| `atlas-decision-log` | Consensus decision logging |
| `atlas-reconfiguration` | System reconfiguration |
| `atlas-metrics` | Performance metrics collection |

The primary ordering protocol used in these benchmarks is [FeBFT](../febft), a PBFT-based consensus protocol with throughput and latency optimizations.

---

## Repository Structure

```
testing-grounds/
├── Makefile                        Top-level entry point for all benchmarks
├── bench/                          Shared benchmarking infrastructure (see bench/README.md)
│   ├── bench.env                   Global configuration defaults
│   ├── Makefile                    Shared parameterized Makefile
│   ├── Dockerfile                  Single shared Dockerfile for all projects
│   ├── config-base/                Config templates (common, replica, client)
│   ├── scripts/                    Config/PKI generation scripts
│   └── playbooks/                  Ansible playbooks for remote deployment
│
├── microbenchmarks-async/          Primary active benchmark: async bounded clients
├── app-scaling-tests/              Application-level scaling experiments
├── preemptive_execution/           Preemptive execution scenario tests
├── hot_stuff/                      HotStuff and Chained HotStuff protocol benchmarks
│   ├── microbenchmark-hotstuff/
│   └── microbenchmark-chainedhotstuff/
├── microbenchmarks/                Legacy synchronous benchmark (inactive — use microbenchmarks-async)
└── random-tests/                   Miscellaneous integration and unit tests
    ├── layered/
    ├── tokrio/
    ├── ycsb/
    └── ...
```

---

## Benchmarks

### `microbenchmarks-async` — Active Primary Benchmark

Measures throughput and latency of asynchronous bounded clients against a placeholder SMR application. Each client runs at most `CONCURRENT_RQS` requests in flight simultaneously and issues `OPS_NUMBER` total operations.

### `app-scaling-tests`

Tests how Atlas scales as the number of clients and replicas increases at the application level.

### `preemptive_execution`

Scenarios testing preemptive execution optimizations in the consensus pipeline.

### `hot_stuff`

Protocol benchmarks for HotStuff and Chained HotStuff implementations built on Atlas.

### `microbenchmarks` (inactive)

Legacy synchronous benchmark. Superseded by `microbenchmarks-async`. Do not use.

### `random-tests`

Ad-hoc integration tests for individual Atlas subsystems: networking, transport, persistent log, ordered maps, YCSB workloads, etc.

---

## Shared Bench Infrastructure

All benchmarks share a single infrastructure under `bench/`. See [`bench/README.md`](bench/README.md) for full documentation.

### Quick Start

```bash
# Run 4 replicas + 1 client locally in Docker
make microbenchmarks-async local

# Override cluster size
make microbenchmarks-async local N_REPLICAS=7 N_CLIENTS=10

# Deploy to a remote cluster via Docker + Ansible
make microbenchmarks-async remote-docker

# Deploy as native binaries (no Docker on remote hosts)
make microbenchmarks-async remote-bare

# Stop a running local deployment
make microbenchmarks-async stop-local

# Emulate a WAN locally (latency/jitter/loss between containers)
make microbenchmarks-async wan-check    # once: verify the kernel has netem
make microbenchmarks-async local WAN_ENABLED=1 WAN_PROFILE=wan-global-3region

# Regenerate configs after changing N_REPLICAS
make microbenchmarks-async gen-configs
```

### Available Projects

| Project | Source directory |
|---|---|
| `microbenchmarks-async` | `microbenchmarks-async/bench` |
| `app-scaling-tests` | `app-scaling-tests/bench` |
| `preemptive_execution` | `preemptive_execution/bench` |
| `microbenchmarks` | `microbenchmarks/bench` |
| `microbenchmark-hotstuff` | `hot_stuff/microbenchmark-hotstuff/bench` |
| `microbenchmark-chainedhotstuff` | `hot_stuff/microbenchmark-chainedhotstuff/bench` |

### Key Configuration Variables

| Variable | Default | Description |
|---|---|---|
| `N_REPLICAS` | `4` | Number of replica processes |
| `N_CLIENTS` | `5` | Logical clients per client machine |
| `CONCURRENT_RQS` | `200` | Max in-flight requests per client |
| `OPS_NUMBER` | `1000000` | Total operations before client exits |
| `REQUEST_SIZE` | `0` | Request payload size in bytes |
| `RUST_LOG` | `INFO` | Log filter for all processes |
| `WAN_ENABLED` | `0` | `1` emulates a WAN between containers in `local` mode |
| `WAN_PROFILE` | `wan-global-3region` | Topology profile from `bench/wan-profiles/` |

### Deployment Modes

- **`local`** — Docker Compose on the local machine. Configs and PKI certificates are volume-mounted. Optionally emulates a WAN (per-link latency, jitter, loss, bandwidth caps) via `WAN_ENABLED=1` — see [bench/README.md](bench/README.md#wan-emulation).
- **`remote-docker`** — Push Docker images to a remote cluster via Ansible. Requires a pre-built image and a populated `hosts.yml`.
- **`remote-bare`** — Cross-compile a native Rust binary and deploy it directly via Ansible. No Docker required on remote hosts.

### Adding a New Project

1. Create `<project>/bench/bench.env` with the four required identity variables (`PROJECT_NAME`, `IMAGE_NAME`, `BINARY_NAME`, `RUST_SRC_RELPATH`).
2. Create `<project>/bench/hosts.yml` for remote deployments.
3. Register the project in `testing-grounds/Makefile`.
4. Verify with `make <project> gen-configs` then `make <project> local`.

---

## PKI / TLS

TLS certificates and Ed25519 signing keys are generated automatically on first run into `bench/generated/ca-root/` using the `atlas-tools` key generation utility. Regenerate after changing `N_REPLICAS`:

```bash
make <project> regen-ca-root
```

---

## Dependencies

- **Rust** (stable toolchain) — for building Atlas binaries
- **Docker** — for local and remote-docker modes
- **Ansible** — for remote-docker and remote-bare deployments
- **InfluxDB 1.8** (optional) — for metrics collection; set `LOCAL_INFLUXDB=1` to spin up a local instance automatically