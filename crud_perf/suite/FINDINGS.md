# Defects found while building the suite

Building a comparison that is fair meant checking the harness for places where it was not, and
the new benchmark for the executors found one real defect in them. Dated 2026-10-31. Each entry
says what was observed, how to reproduce it, what it affects, and its status.

**Fixed**: 1–5, 7, 9–12. **Mitigated**: 8 (about 5% of runs still hang). **Notes**: 6.

---

## 1. No environment variable ever reached `BenchmarkConfig`  *(fixed)*

`config/benchmark_configs.rs` read the environment with `Environment::with_convert_case(Case::Upper)`.
In the `config` crate that runs each key through `convert_case`, where `Upper` means "UPPER CASE"
**with spaces between words**: `EXPENSIVE_OP_SLEEP_MS` became the key `"EXPENSIVE OP SLEEP MS"`,
which matches no struct field. Every knob documented as settable in `bench.env` (`WORKLOAD_TYPE`,
`FORCE_ORDERED`, `KEY_SPACE_SIZE`, the CRUD ratios, key distribution, ...) silently kept its
built-in default.

*Observed:* a smoke run configured with 1 ms per operation reported 3,400 ops/s (impossible for a
sequential executor) and a 15 s measurement window was still running after four minutes.

*Reproduce:* the test `environment_variables_reach_the_benchmark_config`
(`cargo test --bin crud_perf_exec environment_variables`) failed with
`WORKLOAD_TYPE ignored (left: "uniform_cheap", right: "mixed_8020")` before the fix and passes
after it.

*Affects:* older crud_perf results **only** if someone had edited those knobs away from the shipped
defaults. The defaults equal the shipped `bench.env`, so an unedited run measured what it said.

## 2. Command-line knob overrides never reached the containers  *(fixed)*

`make crud_perf local WORKLOAD_TYPE=x` sets a make variable. Containers get their workload knobs
from compose `env_file:`, which Compose reads from the *file*, so the override never arrived and
the run used the file's value. Fixed with a passthrough (`RUNTIME_ENV_PASSTHROUGH` in
`crud_perf/bench/bench.env`, emitted by `gen-local-compose.sh`; `environment:` outranks
`env_file:`). With the environment now actually read (item 1), `CONCURRENT_RQS`, `OPS_NUMBER` and
`REQUEST_SLEEP_MILLIS` had to join the list too: they are also in the `env_file`, and a stale copy
there would beat a command-line override. With no new variables set, the generated compose file is
byte-identical to the original, so other projects are unaffected.

## 3. Mixed and adversarial workloads were not reproducible  *(fixed)*

Their per-request cost was drawn from `fastrand`'s unseeded global generator while keys came from
the seeded one. Two runs, or two executor variants, with the same seed therefore saw different
request streams, which is a confound in precisely the comparison this benchmark exists to make.
Now drawn from the seeded generator; pinned by
`every_workload_is_reproducible_from_its_seed`.

## 4. `crud_scalable` backtracks were invisible to metrics  *(fixed)*

`crud_single` and `dual_state` count backtracks (`CACHE_BACKTRACK_COUNT`, `DS_BACKTRACK_COUNT`);
`crud_scalable`'s `backtrack()` counted nothing and left its cache rebuild untimed. A "no
backtracks happened" check would have passed vacuously for it. It now increments
`CACHE_BACKTRACK_COUNT` and records `CACHE_REBUILD_TIME`, as `crud_single` does (52 scalable tests
still pass).

## 5. The `final-wan` image could not be built  *(fixed)*

The shared Dockerfile's WAN stage runs `apt-get install iproute2` on `debian:bullseye-slim`.
Debian 11 left LTS in August 2026 and `deb.debian.org/debian-security` now returns 404 for its
packages, so no WAN image could be built on this date (the Makefile's `wan-check` probe failed the
same way). Both now point at `archive.debian.org` with `Check-Valid-Until=false`, which keeps the
glibc match with the build stage that the Dockerfile comment asks for. Verified: `make crud_perf
wan-check` reports netem available, the image builds, and on a scratch network the real entrypoint
applying the real `uniform-rtt-50ms` profile measures 50.06 ms replica-to-replica and 0.454 ms
replica-to-client by ping.

## 6. Smaller things

- The client's `request_sleep_millis` is parsed but never used: load is purely closed-loop.
- `State` starts empty, so state size silently depended on how fast a variant warmed up
  (`PRELOAD_KEYS` makes it an input).
- The `crud_perf` README and `dual_state.rs` described a `todo!()` that no longer exists (no
  `todo!()` remains in the crate). Updated.
- `dual_state.rs`'s module doc says the confirmed worker "releases the replies". Reading
  `preemptive_worker/mod.rs`, it is the *preemptive* worker that sends them, at confirmation,
  from the speculative results; the confirmed worker only re-executes. The model in `README.md`
  assumes the code, not the doc. Not changed.

---

## 7. `dual_state`: the executor dies if a backtrack arrives while confirmations are in flight  *(fixed)*

**What happens.** The preemptive worker thread exits, and the confirmed worker then exits on the
disconnected channel. The replica's `dual_state` executor never replies again. Observed error:

```
ERROR ...preemptive_worker: Preemptive worker thread failed with error:
      Failed to confirm preemptive update: No pending preemptive update found for confirmation of seq SeqNo(17): queue empty
ERROR ...confirmed_worker: Confirmed worker failed with error: Recv(ChannelDc { ... }) during normal mode
```

**Mechanism** (inferred from the code and consistent with every observation below).
`handle_backtracking_request` asks the confirmed worker for a copy of its state
(`RequestStateCopy`) and rebuilds the speculative state from it. The confirmed worker handles a
`RequestStateCopy` *immediately*, but executes confirmed batches only at the top of its next loop
iteration (`execute_updates()`), after the whole select round has been handled. If a burst of
`UpdateConfirmed` messages and the `RequestStateCopy` are consumed in the same round, the batches
are queued but not yet executed, so the snapshot is older than the preemptive worker's confirmed
sequence number. `PreemptiveState::backtrack` then resets to that older sequence, and the
re-proposed batch, which is ahead of it, is *staged* behind a gap that will never close instead of
being executed. The confirmation for it finds an empty queue, and the error propagates out of the
worker loop.

**Evidence** (`recovery_bench`, release build, plan `r2_confirmed_backlog`: 10,000 keys, 24 speculated batches of 100 operations, 40 trials per cell):

| confirmed batches in flight | wait before the backtrack | `dual_state` wedged | `crud_single` / `crud_scalable` wedged |
|---|---|---|---|
| 0 | any (0 – 50 ms) | **0 / 320** | none (0 / 320 each) |
| 4 or 16 | 0 ms | **6 / 6** at 0 µs/op and at 20 µs/op (3 consecutive wedges end a configuration) | none |
| 4 or 16 | 2 ms | 0 / 40 at 0 µs/op, **6 / 6** at 20 µs/op | none |
| 4 or 16 | 10 ms or 50 ms | **0 / 40** in every cell | none |

It is a race, and the pattern is what the mechanism predicts. At 20 µs/op, 4 batches of 100 take
8 ms to re-execute, so a 2 ms wait still lands while the confirmed worker is busy and wedges it. At
10 ms with 16 batches in flight (32 ms of work) it does *not* wedge: by the time the request
arrives, the worker has already pushed *and executed* the confirmations, so the copy is current.
A wedge needs the state-copy request and at least one unexecuted confirmation to be consumed in
the same select round, which happens when both arrive while the worker is busy. With zero
processing cost that window is microseconds; with a real per-request cost the worker is busy most
of the time under load, so the window should be wide. **That second half is an inference: I
measured isolated bursts, not a continuous load.** `crud_single` and `crud_scalable` are
unaffected by the same message sequence, and the existing dual-state backtrack test confirms
nothing before backtracking, so it cannot reach this state.

*Correction recorded.* An earlier run of this plan showed five isolated wedges at long waits. They
were the benchmark's: work left in the confirmed worker by the previous (costly) configuration
outlasted the between-trial settle, so the first trial of the next configuration began with
confirmations still in flight. With the pause between configurations that `recovery_bench` now
inserts (scaled to the previous configuration's work), 4 of 4 repeats are clean where 4 of 4
wedged without it, and the plan was re-run. The superseded results are kept beside the new ones as
`results/r2_confirmed_backlog.superseded-harness-leak`.

**Reproduce.**

```bash
cd testing-grounds/crud_perf/crud_perf_exec && cargo build --release --bin recovery_bench
RUST_LOG=error ./target/release/recovery_bench --executor dual_state --state-keys 10000 \
    --depths 24 --positions head --backlogs 16 --trials 3 --timeout-s 5 --out /dev/null
# control: add --pre-trigger-wait-ms 50 -> every trial completes
```

or `suite/plans/recovery/r2_confirmed_backlog.yml` for the full matrix.

**Why it matters.** Backtracks happen on view changes. A replica that loses its executor this way
is silently dead to execution while its consensus module carries on. It also bears directly on the
paper: a recovery comparison in which one candidate crashes has to say so.

**Fix.** In the confirmed worker's `RequestStateCopy` arm, `self.execute_updates()` now runs before
`take_state_snapshot()`. Every `UpdateConfirmed` sent before the request has been queued by then
(the channel is FIFO), so executing the queue makes the copy include them. Pinned by
`test_backtrack_while_confirmations_are_queued_at_the_confirmed_worker` (a slow application keeps
the confirmed worker busy): it failed before the fix with both workers dead, and passes 25 of 25
runs after. The whole crate passes (153; the 2 pre-existing `#[ignore]`d gaps remain ignored), and
the full R2 matrix, which wedged `dual_state` in every immediate-backtrack cell, is now
**960 / 960 ok** with every reply matching the reference model.

## 8. Start-up hang: clients began before the leader had authenticated them  *(mitigated; residual ~5%)*

**What happened.** In 5 of 14 identical LAN runs a cluster ordered nothing: 54,242 consensus
instances in a minute, every one empty, no client request among them. (The earlier
`UnAuthenticatedMessage` drops are a separate, survivable symptom; the hung runs had none.)

**Cause**, from the logs of a hung run. Replicas boot a little apart: replica 3 initialised at
+0.00 s, replica 0 (the leader) at +0.58 s. All five clients were accepted by replicas 1, 2 and 3
(15 of 15 acceptances, none from replica 0), which is a join quorum, so each declared itself ready
at about +0.15 s, before the leader existed, and sent its first window of requests. A request goes
only to nodes the client is connected and authenticated to, so the leader never saw them. Clients
never retransmit (`Client::start_timeout` only sets a `timed_out` flag; the code carries a
`TODO: Repeat the request`), so nothing was ever proposed and every client waited forever.

**Fix.** In `Atlas-Reconfiguration`'s `network_reconfig`, a client that a quorum has confirmed now
waits for *every* bootstrap node to confirm it before reporting the join complete, for at most 10 s
(`CLIENT_STRAGGLER_GRACE`), after which it proceeds on the quorum as before so a replica that is
really down cannot block clients. A straggler's late `NetworkJoinResponse` in the introduction
phase, previously ignored, now counts as a confirmation. Replicas are unchanged. The decision is a
pure function with unit tests.

**Evidence.** The same hunt (`crud_single`, LAN, 3 s warm-up + 5 s measure, `RUST_LOG=info`):

| | runs | hung | runs with start-up message drops |
|---|---|---|---|
| before | 14 | **5** | 2 (132 and 3 messages) |
| after | 20 | **0** | 0 |

If the hang rate were still 36%, 20 clean runs would occur with probability about 0.01%.

**Second cause: replies before the upgrade.** A replica sent its reply to a client before it had
upgraded that client's connection to `authenticated`, and the client's next messages arrived
inside that window. Three Application messages were dropped about 41 µs before the leader's
upgrade. Fix in `Atlas-Communication` (`byte_stub/incoming/pre_auth.rs`): a connection holds
non-Reconfiguration messages while unauthenticated (1024 messages / 2 MiB) and replays them
from `upgrade_connection_to_known`. The flag and the buffer share one lock, so nothing is lost or
duplicated (unit-tested, including a concurrent stress test).

**Evidence with both fixes** (same hunt, `late_client2`, 60-run sweep stopped at 58):
55 ok, **3 hung** (about 5%), plus one run where a client started 10.8 s late (grace expiry).
A window-alignment gate (`window_spread_s`, `MAX_WINDOW_SPREAD`) now excludes the late-start case.

**Still open: the residual 5%.** In each hung run exactly one client ordered nothing. There were
no `UnAuthenticatedMessage` drops and no hold-then-replay events. In `late_client2_0004`, every
replica had upgraded the stuck client 0.6 ms before it sent its first request, so this is not the
auth race. "Too many connections ... Closing connection" churn is *not* the cause: it occurs in
every run (4–104 events), hung or healthy. Untested candidates: the network layer's key or node
update reaching a replica's request pre-processor after the connection upgrade, and the absence
of client retransmission, which turns any single lost request into a permanent hang.

Not found by reading logs; it needs per-request tracing at the replicas' pre-processor. A client
retransmit (`Client::start_timeout` has a `TODO: Repeat the request`) would make it harmless.
A client also starts up to 10 s late if a replica is down.

*Suite behaviour kept.* The runner still detects a stalled run from the clients' `PHASE` markers,
retries it, and records dropped messages, because a hang from some other cause should still be
caught quickly.

## 9. Non-leader replicas never watched client requests  *(fixed in febft; side effect open)*

**What happens.** A request that reaches a non-leader replica but not the leader is never
forwarded and never triggers a view change. In `febft-pbft-consensus/src/bft/proposer/mod.rs` the
non-leader loop `while let Ok(b) = batch_reception.recv()` exits only when `handle_received_message`
returns true, and that returned true only `if is_leader && ...`. So `process_received_messages`, the
only place that calls `watch_received_requests` for a non-leader, was unreachable. Present since
`aa71f06e` (2024-10-01), which made the non-leader wait blocking.

**Evidence.** Plan `plans/fault_first_send.yml`: client 1000's first sends go to one replica only,
replica timeout 1 s (`REQUEST_TIMEOUT_MICROS=1000000`).

| first sends go to | before | after |
|---|---|---|
| all / leader | ok | ok |
| one non-leader replica | never recovered; no replica logged a client-request timeout | ok; that replica forwarded the 3 requests to the leader at +1.00 s |
| nobody | hangs | hangs (expected; clients do not retransmit) |

**Fix.** `handle_received_message` returns true for non-leaders right after accumulating, so each
group of requests is handed to the watcher as it arrives (still blocks in `recv` when idle).

**Side effect, fixed.** With watches armed, non-leaders logged thousands of `handle_timeout` calls
per run that carried 0 timeouts (5,358 in one 40 s run): an ack for a request that was not yet
watched was dropped (`handle_timeout_ack`), so the watch registered afterwards could never be
acked and expired, only to be filtered as already decided. `Atlas-Core`'s timeout worker now
remembers such an ack for up to 5 s (`EARLY_ACK_TTL`, bounded by expiry) and applies it when the
watch registers. Three worker tests pin it (arrives first; below the needed count; expires).
Same plan afterwards: 0 empty calls, only the 3 real timeouts, 3 forwards, throughput unchanged.

**Leader-kill check** (`scenarios/leader_kill.sh`, 1 s timeout, `docker stop replica-0` 8 s into a
40 s window). With the watch armed the first failure path works: +1.00 s survivors forward, +2.00 s
view change, replica 1 leads and proposes. It exposed three more defects (items 10-12) that sat
behind it; with all three fixed all 5 clients finish the window (about 55,000 ops each) and the new
leader decides about 7,500 requests/s.

## 10. The decision log never cleared unfinished decisions on a view change  *(fixed)*

On a view change the replica sends `ClearUnfinishedDecisions`, which called
`clear_decisions_forward(last_executed_seq)`. `DecidingLog::clear_seq_forward_of` treated a sequence
before its first unfinished one as "already decided", logged `Progressed decision that has already
been decided?` and did nothing. The old view's pre-prepare stayed in the instance that the new view
re-ran, and finishing it failed with `Failed to create proof as pre prepare request was not contained
within the provided metadata`, after which nothing was decided. It now clears everything at or after
the given sequence (the trait documents it as inclusive), and the subtraction can no longer underflow.
Four tests on the arithmetic (`entries_to_clear`).

## 11. The proposer used the leadership it read before waiting  *(fixed)*

`info`/`is_leader` were read at the top of the loop, then the thread blocked in `recv()`. With the
leader dead no request arrives, so the thread slept through the whole view change and the first
requests to wake it were treated as non-leader: watched, never proposed. They timed out twice and
caused a second view change. It now re-reads the view after the wait. Verified only by the cluster
scenario; the proposer has no unit-testable seam.

## 12. An epoll worker died when a connection broke during setup  *(fixed)*

`create_connection` read and wrote the new socket and used `?`, so a reset (the peer was just stopped)
returned an error out of `epoll_worker_loop` and ended the thread, taking every connection on it
down: replica 2 lost half its connections at the kill and the cluster stalled. A failed registration,
first read/write or close now drops only that connection (as the event path already did).

**Still open** (none reachable in the scenario after the fixes above):
- A forwarded request the new leader's pre-processor has already seen is discarded
  (`has_received_more_recent_and_update`) even if it was only watched and never proposed.
- A view change cancels every watch; requests that were not in the stopped set are not watched
  afterwards, so nothing times out for them.
- `The view change is not sound. Cancelling.` is logged but nothing is cancelled (a FIXME from
  BFT-SMaRt); the view installs regardless.
- An `AcceptConn` decode error (`invalid value: integer 8, expected variant index 0 <= i < 2`) at
  start-up on replica 1; possibly related to the residual 5% start-up hang (item 8).

