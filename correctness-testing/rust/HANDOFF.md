# correctness-testing — handoff / resume guide

Developer-oriented context for picking this up later. Pairs with:
- **`README.md`** — what the project is + milestone status.
- **`FOLLOW_UPS.md`** — every gap as a tracker-ready entry (FU-1 … FU-14).
- GitHub issues (links in §5) — the same gaps, filed across the four repos.

---

## 1. What this is

An **in-process, protocol-agnostic fault-injection harness** for Atlas ordering protocols.
One Rust process spawns N real `MonReplica` objects (+ clients) as threads, wired by a custom
in-memory "chaos" transport that can drop / sever / crash / omit / delay messages and crash
nodes at exact protocol phases. Scenarios assert **safety** (no fork/equivocation) and
**liveness** (clients keep getting answered when ≤f have failed; halt when > f).

Path: `testing-grounds/correctness-testing/rust`. Run: `cd rust && cargo nextest run`.
(`cargo test` will NOT work — see §4 "serial / nextest".)

## 2. Current state (green)

`cargo nextest run` → **16 passed, 2 skipped**, ~10s (serial). Tests:

| File | Scenario | Protocols |
|---|---|---|
| `bringup.rs` | bootstrap + 1 request agreed | febft, HotStuff, ChainedHotStuff |
| `crash_stop.rs` | crash non-leader → stays live | febft (+ idle-leader-crash `#[ignore]`, FU-1) |
| `leader_crash_phases.rs` | inline crash on leader's COMMIT (F1/F3/F2) | febft |
| `oracle_completion.rs` | 2f+1 completion, no-fault + under crash | febft |
| `omission.rs` | send/receive/filtered omission | febft |
| `amnesia.rs` | equivocation negative control; crash→restart→recover no-equivocation (+ double-vote `#[ignore]`, FU-1) | febft |
| `simultaneous_crash.rs` | crash f → stays live | febft |
| `halt_not_fork.rs` | crash f+1 → halts, no fork | febft |
| `restart.rs` | crash → `restart_amnesia` → rejoin | febft |
| `state_transfer_checkpoint_reconfig.rs` | recovered node catches up (`op_count` agreement) | febft |

Milestones: **M0–M3 + M5 done**; **FU-6 (node restart) done**; **M4 partial** (recovery/catch-up
done; checkpoint + reconfig crashes blocked, filed). Fault/load scenarios are febft-only because
HotStuff/IronChain hit protocol-internal issues under faults/load (FU-2/FU-3).

## 3. Architecture map

```
src/
  chaos/            in-process transport (implements Atlas byte-network traits)
    mesh.rs           ChaosMesh: THE shared broker. Owns adjacency + severed set +
                      rules + gate observer. dispatch() is the outbound gate (runs on the
                      threadpool worker — sends are async, F1). Delivers inline via a
                      type-erased `NodeDelivery` per node.
    controller.rs     ChaosByteController<NI,IS,NSC> (= replaces atlas_comm_mio::MIOTCPNode)
                      + NodeEndpoint (holds a node's PeerConnectionManager; generates the
                      inbound stub per peer via generate_stub_for).
    stub.rs           ChaosByteStub (per-edge outbound; dispatch_blocking → mesh.dispatch).
    conn_controller.rs ChaosConnectionController: has_connection / connect_to_node delegate
                      to the mesh (F2). connect_to_node returns a pre-fired Ok oneshot.
    rules.rs          EdgeRule / RuleAction (Drop, CrashNode) / RuleSet; inline crash at gate.
  lifecycle/
    identity.rs       ClusterIdentity: DISTINCT per-node Ed25519 keys, seed=f(node_id) (F6).
    config.rs         TimingConfig only (per-protocol config built in backend.rs).
    backend.rs        ProtocolBackend trait (NodeConfig; generate_node_configs;
                      bootstrap_and_run) + Febft / HotStuff / ChainedHotStuff impls.
                      RestartableBackend (only Febft) gates restart.
    node_handle.rs    NodeHandle: spawn::<P> / graceful_stop / true_crash.
    cluster.rs        ClusterHarness<P>: the object scenarios talk to. Holds the ONE
                      InitGuard; concurrent bring-up; new_client / submit_ordered / crash /
                      restart_amnesia / restart_with_state / set_observer / install_rule.
  composition/        the SMR generic type soup, split:
    mod.rs            shared (app / client side / state-transfer msg / byte layer). Client
                      side has NO ordering-protocol param → shared across protocols.
    febft.rs, hotstuff.rs, chained.rs   per-protocol replica composition.
  adapters/febft.rs   febft byte-layer CONTENT predicates (deserialize WireMessage →
                      PBFTMessage; recognize COMMIT/PREPARE/PRE-PREPARE/view-change) +
                      crash_leader_before_commit_broadcast + record_votes observer.
  oracle/
    completion_ledger.rs  CompletionLedger (Pending/Completed/Failed).
    client_pool.rs        ClientPool over ConcurrentClient; assert_all_completed / assert_pending.
    vote_log.rs           VoteLog + equivocation detection.
    reply_log.rs          ReplyLog: records replica→client replies (app-level, protocol-agnostic)
                          → execution-level fork detection.
  apps/kv_echo.rs     deterministic monolithic app (op_count in state — used to check catch-up).
  lib.rs              re-exports + protocol_test! matrix macro.
tests/                one file per scenario; matrix scenarios use protocol_test!.
```

Data flow of one message: protocol `send` → (async threadpool) `ChaosByteStub::dispatch_blocking`
→ `ChaosMesh::dispatch(from,to,msg)` [severed check → observer → rules → deliver] →
destination `NodeEndpoint::deliver` → `generate_stub_for(from)`'s incoming stub
`handle_message(&dest_network_info, msg)`. Loopback (self→self) bypasses the gate (F3).

## 4. Load-bearing design decisions & gotchas (READ before changing anything)

- **`cargo nextest` REQUIRED; `test-threads = 1`.** `atlas-common` has process-global `OnceLock`
  singletons (threadpool/runtime/metrics) — one `atlas_common::init` per process, guard held for
  the scenario. So one scenario = one process. Serial because a finished test leaves protocol
  threads briefly spinning (disconnected-channel loops, FU-5/FU-7) that starve a concurrent heavy
  bootstrap (IronChain most sensitive). `.config/nextest.toml` sets `test-threads = 1` +
  120s hard-kill.
- **`[profile.dev] overflow-checks = false`** (Cargo.toml). Atlas/febft are only run in release;
  febft's proposer has an unchecked subtraction underflow (FU-4) that panics under dev checks.
- **Executor = `SingleThreadedMonExecutor<AppNetwork>`, NOT `MonolithicPreemptiveExecutor`.** The
  preemptive one's worker dies on startup in-process (channel disconnect) → decided requests never
  execute. Decision log = `Boule` (`atlas_decision_log::Log` no longer exists).
- **Transport swap = 2 type aliases.** Only `ByteNetworkLayer` (→ `ChaosByteController`) and
  `ByteStubType` (→ `ChaosByteStub`) differ from the stock MIO composition; everything above is
  generic. Client side is fully protocol-agnostic (no ordering-protocol type param).
- **Distinct per-node keys (F6).** `identity.rs` seeds each node differently; the workspace mocks
  seed all nodes from `[0;32]` (identical keys) — which would make equivocation checks meaningless.
- **Bring-up must fire all N bootstraps concurrently** — `Replica::bootstrap` blocks on the
  reconfiguration protocol reaching a stable quorum (`server/mod.rs:459`); serial spawn deadlocks.
- **`true_crash` severs in the mesh AND flips the stop trigger** (F2). `has_connection` is really
  queried by reconfiguration, so the mesh must model a crashed node as disconnected.
- **HotIron/IronChain need 4 generic args** `<RQ, Network, QuorumInfo, RequestPreProcessor>`
  (microbenchmark's 3-arg form is stale) and threshold-crypto configs from `QuorumInfo::initialize(f)`
  (not `Clone` → they can't be restarted; only febft implements `RestartableBackend`).
- **febft leader is view-scoped** (`quorum[view_seq % n]`), initial primary = node 0 — not
  `seq % n` of the consensus seq. `ClusterHarness::initial_leader()` returns node 0.

## 5. Missing implementations / open issues

All filed on GitHub (owner `nuno1212s`). Grouped:

**Protocol / upstream (block the fault matrix across all 3 protocols):**
- `febft#23` (FU-1) — no view change on idle leader crash. Unblocks `crash_stop_leader_idle_recovery`
  + amnesia double-vote + leader-crash-phase liveness assertions.
- `febft#24` (FU-4) — proposer subtraction underflow (`saturating_sub`). Removes the dev workaround.
- `hot-iron-oxide#1` (FU-2) — HotStuff/IronChain crash-stop liveness stall.
- `hot-iron-oxide#2` (FU-3) — IronChain "provided QC does not match the decision node header"
  under load (likely `Boule`-vs-IronChain decision-log mismatch).
- `Atlas#7` (FU-5) — state-transfer thread busy-loops on a disconnected channel (fixes FU-7 too).
- `Atlas#8` (FU-12) — expose `CHECKPOINT_PERIOD` as a test override (F7). Unblocks FU-13.

**Harness (this project — actionable here):**
- `testing-grounds#9` (FU-6) — node restart. ✅ DONE.
- `testing-grounds#10` (FU-7) — serial-only / teardown thread leaks. Make teardown join all
  per-node protocol threads before dropping the InitGuard; then raise `test-threads`.
- `testing-grounds#11` (FU-9) — fault scenarios febft-only; extend each `protocol_test!(... => [...])`
  list once FU-2/FU-3/FU-10 land (one line each).
- `testing-grounds#12` (FU-10) — `adapters/hotstuff.rs` + `adapters/chained_hotstuff.rs` content
  predicates (per-adapter crash-points, F5). Mirror `adapters/febft.rs`.
- `testing-grounds#13` (FU-11) — stronger safety: `assert_no_conflicting_decisions` via direct
  decision-log inspection (today's fork check is the reply-based proxy).
- `testing-grounds#14` (FU-13) — M4 checkpoint / snapshot-transfer crash scenarios (blocked on Atlas#8).
- `testing-grounds#15` (FU-14) — M4 reconfiguration-coordinator crash (LockedQC→commit); needs a
  `ReconfData` content-predicate adapter.

**Deferred (plan §Future work):** IronDumbo adapter — needs IronDumbo's base SMR integration
(`MonReplica` wiring, `CollabStateTransfer`, transfer/reconfig crates) which doesn't exist yet.

## 6. Suggested resume order

1. **`ReconfData` adapter (FU-14)** and **HotStuff/IronChain adapters (FU-10)** — pure harness work,
   no upstream dependency; unblock reconfig-crash + phase-precise faults on the other protocols.
2. **Teardown join (FU-7)** — lets the suite parallelize; small, self-contained.
3. Upstream fixes you own (FU-1/2/3/5) — each flips one or more `#[ignore]`/febft-only scenarios green.
4. **Atlas#8 (FU-12)** then **FU-13** — the checkpoint/snapshot-transfer crash sub-cases.
5. FU-11 (decision-log inspection) — strengthens the halt-not-fork safety assertion.

The full adversarial design analysis and finalized plan are at
`~/.claude/plans/correctness-testing-FINAL-plan.md`. Verified gotchas are also in the project
memory note `correctness-testing-framework.md`.
