# correctness-testing

An **in-process, protocol-agnostic fault-injection harness** for Atlas ordering
protocols. Unlike the other `testing-grounds` projects (which measure *performance* via
Docker/Ansible over real OS processes and sockets), this harness spawns a whole BFT
cluster — N real `MonReplica` objects plus clients — as **threads inside one process**,
wired together by a custom in-memory *chaos transport*. That trades OS-process realism
for determinism and the ability to inject faults at **exact protocol phases** (something
`docker kill` timing can never achieve).

The interesting BFT/CFT bugs live at fault boundaries — a leader crashing after a quorum
of votes but before commit is observed, a node recovering without replaying its WAL and
equivocating. This harness exists to exercise exactly those, and to assert that survivors
stay **safe** (no forking/equivocation) and, when ≤f nodes have failed, stay **live**.

## Running

```bash
cd rust
cargo nextest run
```

**`cargo nextest` is REQUIRED — plain `cargo test` will not work.** `atlas-common` holds
process-global `OnceLock` singletons (threadpool, async runtime, metrics) that can never
be reset within a process. Each scenario calls `atlas_common::init(...)` exactly once and
holds the `InitGuard` for its lifetime; a second scenario in the same process would hit
`InitPoolError`. nextest gives every `#[test]` its own OS process, so **the unit of
process isolation is one scenario**.

## Layout

```
rust/
  src/
    chaos/       in-process byte transport (ByteNetworkController/Stub/ConnectionController)
    lifecycle/   PKI synthesis, node spawn/crash/restart, ClusterHarness
    apps/        deterministic workload applications (kv_echo)
    composition.rs   the concrete febft SMR generic stack (swaps only the byte layer)
  tests/
    bringup.rs   M0 gate: N replicas + client agree on one request, no faults
```

## `bench/` registration — intentional deviation

This project is registered in the top-level `Makefile`'s `PROJECTS` list **only** so
`make correctness-testing build-binary` is discoverable for CI. The `local` /
`remote-docker` / `remote-bare` deployment modes are **deliberately NOT wired** — there is
no Docker image or Ansible target, because this is an in-process harness. `bench/hosts.yml`
is an empty stub that exists only to satisfy the shared Makefile's dir-exists check.

## Status

- **M0 — bring-up gate:** ✅ 4 real `MonReplica`s + a client bootstrap in-process and agree on
  one request, no faults.
- **M1 — foundation faults (febft):** ✅ crash-stop (F2), the F1 inline-gate fault engine with
  content predicates (F3), and phase-precise leader crash.
- **M2 — completion oracle + hard scenarios (febft):** ✅ 2f+1 completion ledger (F4),
  send/receive/filtered omission, and the vote-observer equivocation oracle (negative control).
- **M3 — protocol generalization:** ✅ febft + HotIron + IronChain compile together and all
  **bootstrap + agree** in-process (`protocol_test!` matrix); the `ProtocolBackend` trait +
  per-protocol composition modules. Fault/load scenarios currently run febft-only.
- **M5 — f+1 halt-not-fork:** ✅ simultaneous crash of f stays live; crash of f+1 halts (post-
  crash requests correctly never complete) without forking (execution-level fork check via the
  byte-gate reply observer). febft.
- **Node restart (FU-6):** ✅ `restart_amnesia` (fresh db) / `restart_with_state` (same db) —
  a crashed replica rejoins the quorum in-process; used by the amnesia recovery scenario.
- **M4 (state-transfer / checkpoint / reconfiguration crashes):** 🟡 partial — recovery/catch-up
  is done (a crashed node rejoins with a fresh db and catches up via peer-driven transfer,
  verified by `op_count` agreement). The checkpoint-crash / state-snapshot-crash (needs an Atlas
  checkpoint override, F7) and reconfiguration-coordinator crash (needs a reconfig predicate)
  sub-cases remain blocked. See `FOLLOW_UPS.md` (FU-12/13/14).

**Resuming this work later?** Start with [`HANDOFF.md`](HANDOFF.md) — architecture map,
load-bearing design decisions/gotchas, the open-issue list, and a suggested resume order.
Known gaps and everything that doesn't yet work are tracked in
[`FOLLOW_UPS.md`](FOLLOW_UPS.md) (tracker-ready) and as GitHub issues. The suite runs serially
(`test-threads = 1`) for reliability — see FU-7.
