# correctness-testing — follow-up issues

Findings from building M0–M3 that do **not** currently work and were deliberately scoped
out (documented gaps, `#[ignore]`d tests, or febft-only matrix entries). Each entry is
written to be pasted into an issue tracker. Grouped by owner:

- **§1 Protocol / upstream Atlas** — behavior in febft / hot-iron-oxide / atlas-* that the
  harness surfaces but does not own.
- **§2 Harness (this project)** — actionable within `correctness-testing`.
- **§3 Coverage** — scenarios/protocols not yet wired, blocked by the above.

Status legend: 🔴 blocks a scenario · 🟡 workaround in place · 🟢 cosmetic.

---

## §1 Protocol / upstream Atlas findings

### FU-1 🔴 febft: no liveness recovery after an *idle* leader crash
- **Where:** `tests/crash_stop.rs::crash_stop_leader_idle_recovery_known_gap` (`#[ignore]`).
- **Symptom:** crash the leader (node 0) when the cluster is idle (the baseline request has
  already committed), then submit a request → it never completes (hangs to the 120s ceiling).
- **Evidence (verified):** the chaos transport delivers the post-crash request to all three
  survivors (gate trace: `Application`-module message to nodes 1/2/3). So it is *not* a
  transport bug.
- **Root cause:** febft only ever arms a client-request timeout via
  `ReplicaSynchronizer::watch_received_requests`
  (`febft/febft-pbft-consensus/src/bft/sync/replica_sync/mod.rs:181`), which is reached from
  the proposer (`.../bft/proposer/mod.rs:242`) *only after* its inner
  `while let Ok = batch_reception.recv()` loop breaks. For a **non-leader** that loop never
  breaks (`handle_received_message` returns `false` unless `is_leader`,
  `.../bft/proposer/mod.rs:339`), so non-leaders accumulate requests but never watch them.
  febft consensus registers no `SeqNoBased` timeouts (only cancels). With no in-flight
  consensus instance at crash time, nothing triggers a view change.
- **Fix / owner:** febft view-change trigger for the idle-leader-crash case (protocol domain).
  When resolved: drop the `#[ignore]`, and the interesting *in-flight* leader-crash-at-phase
  scenario (crash the leader as it broadcasts COMMIT — machinery already built in
  `leader_crash_phases.rs`) can assert liveness + safety instead of mechanism-only.

### FU-2 🔴 HotStuff & IronChain: crash-stop liveness stalls in-process
- **Where:** `crash_stop_non_leader_stays_live` is matrixed to `[Febft]` only.
- **Symptom:** the same scenario times out on `ChainedHotStuff` (and `HotStuff`) **even when
  run serially** — crashing a non-leader does not leave the cluster live.
- **Root cause:** same class as FU-1 (fault-driven recovery not triggered in-process).
  Protocol domain (hot-iron-oxide).
- **Fix / owner:** hot-iron-oxide fault-recovery/timeout path; then extend the
  `protocol_test!(crash_stop_non_leader_stays_live => [...])` list.

### FU-3 🔴 IronChain: "provided QC does not match the decision node header" under load
- **Where:** `oracle_all_requests_complete_no_fault` matrixed to `[Febft]` only.
- **Symptom:** on `ChainedHotStuff`, a 20-request batch never fully completes; the decision-log
  thread spams `ERROR atlas_smr_replica::server::decision_log: ... The provided QC does not
  match the decision node header` (~176k lines in 40s). Single-request bringup
  (`bringup_one_request::ChainedHotStuff`) is fine — only sustained load triggers it.
- **Suspected cause:** IronChain paired with the `Boule` decision log. The
  microbenchmark-chainedhotstuff reference used `atlas_decision_log::Log`, which **no longer
  exists** in the current Atlas (only `Boule`), so this harness uses `Boule` for all
  protocols. `Boule` may be the wrong decision log for IronChain's pipelined QCs, or there is
  an IronChain decision-log integration bug.
- **Fix / owner:** determine the correct decision-log type for IronChain against current Atlas
  (or fix the QC/header matching). Protocol / decision-log domain.

### FU-4 🟡 (upstream) febft proposer: unchecked subtraction underflow panics under debug
- **Where:** `febft/febft-pbft-consensus/src/bft/proposer/mod.rs:374` —
  `global_batch_time_limit - propose.last_proposal.elapsed().as_micros()`.
- **Symptom:** underflows (panics) the moment the proposer idles past one batch window, when
  built with overflow checks on (default `cargo test`/dev). Benchmarks never hit it because
  they only run `release` (`overflow-checks = false`).
- **Workaround in place:** `[profile.dev] overflow-checks = false` in this crate's `Cargo.toml`.
- **Fix / owner:** use `saturating_sub` upstream in febft; then the workaround can be removed.

### FU-5 🟡 (upstream) atlas-smr-replica state-transfer thread busy-loops on a disconnected channel
- **Where:** `Atlas/Atlas-SMR-Replica/src/server/monolithic_server/state_transfer.rs:100-104`
  — `loop { if let Err(err) = self.run() { error!("Received state transfer error {:?}", err) } }`.
- **Symptom:** when any of its `select` channels is disconnected (notably during teardown, or
  when the preemptive executor's worker dies), `run()` returns `Err` immediately and the outer
  loop spins at 100% CPU, flooding errors. This is the main contributor to FU-9 (teardown
  starvation).
- **Fix / owner:** back off / exit the loop on a terminal `Disconnected` error upstream.
- **Note:** originally surfaced with the preemptive executor (its worker died on startup →
  dropped the checkpoint sender → this loop spun). This crate switched to
  `SingleThreadedMonExecutor`, which avoids the *startup* trigger, but the teardown trigger
  remains.

---

## §2 Harness (this project) — actionable

### FU-6 ✅ DONE — `restart_amnesia` / `restart_with_state` implemented
- `ClusterHarness<P>::restart_amnesia` (fresh empty db) and `restart_with_state` (same db),
  gated on `P: RestartableBackend` (a marker sub-trait; only `Febft` implements it — the
  HotStuff family's `QuorumInfo` shares aren't `Clone`/reproducible). Flow: crash the old
  handle, brief unwind delay, `mesh.unsever`, spawn a fresh bootstrap thread, wait for ready.
- **Verified:** `tests/restart.rs` — a crashed node rejoins the quorum with a fresh db and the
  cluster stays usable. `tests/amnesia.rs::amnesia_restart_recovers_without_equivocation` —
  crash → `restart_amnesia` → recover, with the vote oracle confirming no equivocation.
- **Remaining:** the DANGEROUS amnesia double-vote (`amnesia_double_vote_known_gap`, still
  `#[ignore]`) — forcing the recovered node to *re-vote* a still-undecided seq needs a view
  change on the in-flight sequence (blocked by FU-1, not by restart anymore).

### FU-7 🟡 Suite must run serially (`nextest test-threads = 1`)
- **Where:** `.config/nextest.toml`.
- **Symptom:** at `test-threads > 1`, a *finished* test's leaked, still-spinning protocol
  threads (the FU-5 state-transfer loop; abandoned `true_crash` threads) starve a concurrent
  heavy bootstrap — IronChain's is the most sensitive and stalls to the 120s ceiling. Even
  febft bringup stalled at high concurrency.
- **Root cause (harness side):** `ClusterHarness::drop` graceful-stops the replica *main*
  threads but does not join the per-node protocol threads (reconfig / state-transfer /
  decision-log / proposer / timeout), and `true_crash` deliberately abandons threads. On
  `InitGuard` drop these race and some busy-loop (FU-5).
- **Workaround in place:** serial execution (the suite is sub-second serially, so cheap).
- **Fix:** make teardown deterministic — signal all per-node protocol threads to stop and join
  them before dropping the `InitGuard`; depends partly on FU-5 upstream. Then raise
  `test-threads`.

### FU-8 🟢 HotStuff/IronChain BLS threshold crypto is CPU-heavy
- Threshold-crypto sign/verify per request is far heavier than febft's Ed25519. Compounds FU-7
  under parallelism. Informational; mitigated by `test-threads = 1`.

---

## §3 Coverage not yet wired (blocked by §1)

### FU-9 🔴 Fault/load scenarios are febft-only across the matrix
- `crash_stop`, `oracle_*`, `omission`, `leader_crash_phases`, `amnesia` run on febft only;
  only `bringup_one_request` runs across `[Febft, HotStuff, ChainedHotStuff]`.
- Blocked by FU-2 (crash-stop liveness) and FU-3 (IronChain decision log). Extending each
  `protocol_test!(... => [...])` list is a one-line change once those are resolved.

### FU-10 🔴 HotStuff / IronChain content-predicate adapters not implemented
- **Missing:** `adapters/hotstuff.rs` and `adapters/chained_hotstuff.rs` (only `adapters/febft.rs`
  exists).
- **Needed for:** phase-precise leader crash, content-filtered omission, and the vote-observer
  equivocation check on HotStuff/IronChain. Per F5 the crash-point is a *per-adapter* concept
  (HotIron uses leader-directed vote/QC events, not febft's per-replica COMMIT; IronChain is
  seq-scoped/pipelined), so these are genuinely separate classifiers, not renames of febft's.
- **Shape:** deserialize the `Protocol`-module payload with `ProtocolDataType` and match on the
  `HotIronOxSer` / `IronChainSer` message enums (mirror `adapters/febft.rs`).

---

### FU-11 🟡 Stronger safety check: direct decision-log inspection
- **Now:** halt-not-fork's no-fork check (`halt_not_fork.rs`, `oracle/reply_log.rs`) is an
  *execution-level* proxy — it flags two replicas reporting different **replies** for the same
  client op. This is observable at the gate and catches real forks, but it only sees seqs that
  produced client-visible replies.
- **Wanted:** `assert_no_conflicting_decisions` — inspect each surviving replica's **decision
  log** for a given seq and assert byte-equality across replicas (the plan's stronger form).
- **Challenge:** a running `MonReplica` owns its decision log on its thread; the harness has no
  handle to it. Options: expose a read handle, or read the persisted decision log from each
  node's `db_path` after the scenario (`read_decision_log`, as bootstrap does at
  `Atlas-SMR-Replica/src/server/mod.rs:350`). Related to FU-6 (restart also needs log access).

### FU-12 🔴 (Atlas) Expose the checkpoint period as a test override (F7)
- febft/Atlas take a checkpoint every `CHECKPOINT_PERIOD` (a const consumed deep in the decision
  log; see the `//TODO: Move this to an env variable` at `Atlas-SMR-Replica/src/server/mod.rs:107`).
- In a short in-process test, a checkpoint never fires, so a **snapshot** state-transfer (as
  opposed to log replay) is never exercised. This blocks the M4 checkpoint-crash and
  state-snapshot-transfer-crash scenarios (FU-13).
- **Ask:** make the checkpoint period test-settable — an env var (or process atomic) read at
  `probe_checkpoint_needed`, smallest blast radius (per F7). Test-only infra.

### FU-13 🔴 M4 checkpoint-crash / state-snapshot-transfer-crash scenarios
- Force frequent checkpoints (via FU-12), then crash a node mid-checkpoint, and crash a lagging
  node mid-**snapshot** state-transfer. Blocked on FU-12. (The recovery/catch-up via *log* transfer
  is already done — `state_transfer_checkpoint_reconfig.rs::recovered_node_catches_up_and_agrees`.)

### FU-14 🔴 M4 reconfiguration-coordinator crash (LockedQC → commit)
- Crash the reconfiguration coordinator between locking the `LockedQC` and committing the
  `CommittedQC` (the two-phase scheme at `Atlas-Reconfiguration/src/message/mod.rs:203-228`),
  protocol-agnostic.
- **Needs:** a reconfiguration-message content predicate (a `ReconfData` adapter — mirror
  `adapters/febft.rs` but for `atlas_reconfiguration::message`), to recognize the LockedQC/
  CommittedQC window at the gate for a phase-precise inline crash.

## Milestone status (for reference, not defects)

- **M0–M3, M5:** ✅ implemented and green (see `README.md`).
- **M4** — 🟡 **partial**: the recovery / catch-up sub-case is done and green (a crashed node
  rejoins with a fresh db and catches up, verified via `op_count` agreement). The
  checkpoint-crash / state-snapshot-transfer-crash sub-cases (FU-12/FU-13) and the
  reconfiguration-coordinator crash (FU-14) remain blocked. FU-6 (node restart), the main
  harness prerequisite, is done.
