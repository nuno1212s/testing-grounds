# crud_perf experiment suite

Two questions, two sets of experiments.

| | Question | Instrument | Experiments |
|---|---|---|---|
| **Part 1** | How much does speculative execution help, and how does that change with the network latency between replicas and with the processing cost of a request? | Full cluster in Docker, tc/netem between replicas | E1 – E4 |
| **Part 2** | When speculation turns out to be wrong, how long does each speculative executor take to recover, and what does that depend on? | In-process micro-benchmark of the executors themselves | R1 – R5 |

Everything is driven by a plan file (`plans/`), a plan plus the git revisions it ran against is a
complete description of an experiment, and every result carries both.

```
suite/
  run_suite.py          Part 1 runner      (plan | run | status)
  analyze_suite.py      Part 1 tables      -> report.md, summary.csv, runs_flat.csv
  run_recovery.py       Part 2 runner      (plan | run)
  analyze_recovery.py   Part 2 tables      -> report.md, summary.csv
  gen_wan_profiles.py   writes bench/wan-profiles/uniform-rtt-*.yml
  plans/                E1-E4, a pilot, smoke tests          plans/recovery/  R1-R5
  lib/                  histogram decoding, t-intervals, plan expansion, InfluxDB
  FINDINGS.md           defects found while building this, with reproductions
  tests/                python3 -m unittest discover -s tests
```

## Quick start

```bash
cd testing-grounds/crud_perf/suite

# Part 1 ----------------------------------------------------------------------------------
./gen_wan_profiles.py                          # once; the files are committed
make -C ../.. stop-metrics                     # once: WAN mode needs atlas_network on its own
                                               #   subnet and the metrics stack holds the default one
make -C ../.. crud_perf wan-check              # once: does this kernel have sch_netem?
./run_suite.py plan plans/e1_pilot.yml         # what would run, how long, and preflight checks
./run_suite.py run  plans/e1_pilot.yml         # 36 runs, ~1.3 h: a first look at the shape
./analyze_suite.py results/e1_pilot            # -> results/e1_pilot/report.md

# Part 2 ----------------------------------------------------------------------------------
(cd ../crud_perf_exec && cargo build --release --bin recovery_bench)
./run_recovery.py run plans/recovery/r1_state_size.yml
./analyze_recovery.py results/r1_state_size
```

Interrupt a sweep with Ctrl-C or `kill`; it tears the cluster down, and `--resume` continues from
the next run. A run whose clients stall is failed within about a minute of the overdue phase
boundary (the clients announce `PHASE warmup_start` / `measure_start` at WARN, so the runner can see
progress under `RUST_LOG=warn`) and retried twice (`FINDINGS.md`, item 8). `run_suite.py run --quick` runs the first and last
level of every factor once, which is the right shape for checking a new plan.

Before trusting a plan on a new host, run `plans/smoke.yml` (WAN) and `plans/smoke_lan.yml` (no
WAN, needs no change to the network). They exercise the whole path in minutes; their numbers mean
nothing.

---

# Part 1: speculative against normal execution

## What the model predicts

PBFT needs two further message rounds after a replica receives the leader's pre-prepare: prepare,
then commit. With a uniform one-way delay `d` between replicas that is `2d`, one replica round
trip, `RTT`. The baseline executor starts a batch when the decision arrives. A speculative
executor starts it when the pre-prepare arrives, so it can overlap its execution with those two
rounds, and no more. Per batch:

```
latency saved  =  min( execution time of the batch ,  RTT between replicas )
execution time =  batch size × per-request cost            (crud_scalable: ceil(batch / 4) × cost)
```

So the benefit should be **absent when either factor is near zero** (nothing to overlap, or
nothing to overlap it with), should **grow along the diagonal** where the two are comparable, and
should **saturate at one RTT** however expensive the request. `analyze_suite.py` prints the
measured saving beside this prediction using each run's own measured batch size. It ignores
queueing between pipelined batches, so read it as a guide to shape and ordering, not a fit.

## Experiments

| Plan | Varies | Fixed | Runs | Time* |
|---|---|---|---|---|
| `e1_latency_processing` | replica RTT {0.4, 5, 20, 50, 100, 200} ms × request cost {0, 0.1, 1, 5, 20} ms | 5 clients × 3 outstanding | 360 | ~13 h |
| `e1_pilot` | RTT {5, 50, 200} × cost {0, 1, 20} ms, one repetition | as E1 | 36 | ~1.3 h |
| `e2_load_scaling` | outstanding requests per client {1 … 128} under RTT {0.4, 50} × cost {0, 1} ms | | 192 | ~7 h |
| `e3_cost_model` | `sleep` against `spin`, cost {0.1, 1, 5} ms | RTT 50 ms | 72 | ~2.6 h |
| `e4_contention` | key space {10⁶, 10³, 100, 10} (write-heavy mix) | RTT 50 ms, cost 1 ms | 48 | ~1.7 h |

\* 4 variants × 3 repetitions (E2: 2), 10 s warm-up + 45 s measurement + ~75 s boot/teardown per run.
All four variants run in every cell: `baseline`, `dual_state`, `crud_single`, `crud_scalable`.

**E1** is the headline. **E2** answers "how does it scale": at low load speculation hides latency;
toward saturation the question is whether its extra work (re-execution in `dual_state`, delta
bookkeeping in the CRUD variants) costs throughput. **E3** checks that the conclusions do not
depend on how processing cost is modelled. **E4** tests the one place `crud_scalable` should lose:
it runs a batch in parallel and then re-executes colliding operations serially.

## What is held identical, and where that is enforced

A comparison is only as good as what it holds constant. Each row is a way two variants could
otherwise have differed.

| Could differ in… | Held fixed by | Enforced in |
|---|---|---|
| Code | One binary; the executor is a compile-time feature, everything else is shared. The image tag encodes the variant and the run records which image each container ran. | `executor_variant/`, `analyze_suite.py:validity` |
| The requests | Per-client seeded generators. The mixed/adversarial workloads drew delays from an *unseeded* global RNG, so the same seed gave a different stream each run (fixed; pinned by a test). | `workload/mod.rs` |
| Which requests reach consensus | `FORCE_ORDERED`: an unordered read bypasses consensus and is untouched by speculation, so including it dilutes the effect. | `_base.yml` |
| The starting state | `PRELOAD_KEYS`. Without it every run starts empty and grows during warm-up, so a *faster* variant would be measured against a *bigger* state. | `knobs.rs`, `exec.rs` |
| Hardware | Compose `cpuset:`: each node gets a private block of cores (8 per replica, 8 for the client), identical for every variant. A `cpus:` quota is deliberately avoided: CFS throttling in 100 ms periods looks exactly like the latency being measured. | `gen-local-compose.sh` |
| The network | Generated uniform-RTT profiles; every replica pair has the same RTT, clients are 0.4 ms from every replica, no jitter (netem reorders under jitter), no loss. The RTT is **measured** by ping inside the cluster each run and the run is excluded if it is off by more than 15%. | `gen_wan_profiles.py`, `run_suite.py:measure_rtts` |
| Measurement window | Time-based phases (`WARMUP_SECS`/`MEASURE_SECS`). A fixed op count is seconds at LAN speed and tens of minutes at 200 ms, and gives a faster variant a shorter, noisier window. | `client.rs` |
| The result path | Latency comes from an in-process histogram printed by the client, not from InfluxDB. A distribution reassembled from a metrics database that drops samples under load is not a measurement. | `histogram.rs` |
| Metrics load | `CLIENT_REQUEST_METRICS=0`: per-request InfluxDB points would only load the node. | `_base.yml` |
| Drift (thermal state, caches, neighbours) | Runs are ordered as **randomised blocks**: one block per (cell, repetition), all variants back to back in shuffled order, blocks shuffled. Savings are computed **paired within a block**, so slow drift cancels in the difference instead of widening two independent intervals. | `lib/plan.py`, `analyze_suite.py:paired_saving` |
| Small-sample error | Student-t 95% intervals (3 runs need 4.30 standard errors, not 1.96). p99 is shown only with ≥ 1000 samples per run. | `lib/stats.py` |

### Validity gates

A run is excluded from the statistics, and listed with the reason, if it fails any of:

- **speculation did not carry the run** (`SPECULATION_HIT_RATE` < 900‰ on a speculative variant).
  This is the important one: a speculative variant that silently fell back to post-commit
  execution has measured the baseline against itself, and its latency is a correct measurement of
  the wrong thing;
- any **backtrack** in a fault-free run (all three speculative executors now count them);
- a **replica restarted** (`restart: on-failure` would otherwise hide a crash);
- the **image** does not match the variant, or the **measured RTT** is off the profile;
- fewer clients reported than were started, or no operations completed;
- client measurement windows that start more than 10% of the window apart (throughput sums per-client
  rates, which is only valid if the windows overlap).

Valid-with-warning runs (for example the start-up race below) are kept and listed.

### Cost model: `sleep` against `spin`

A request's processing cost is charged on the replica by `PROCESSING_MODE`. `sleep` yields the
thread, so it costs latency but no CPU and a parallel executor scales as far as its pool allows
whatever the core count. `spin` holds a core, the only mode in which an executor's parallelism is
bounded by the cores it is actually given. The suite defaults to `spin`; E3 reruns a slice under
`sleep` to show whether the conclusion depends on it. `thread::sleep` also overshoots by tens of
microseconds, a large relative error below 1 ms.

Note `crud_scalable` uses a **hard-coded 4-thread** execution pool
(`PARALLEL_EXEC_THREAD_POOL_SIZE`), so its parallel speedup is capped at 4× whatever the
hardware; that is why there is no "core scaling" experiment (it would be flat beyond 4).

## Threats to validity

- **One host.** The "WAN" is netem on a bridge between containers on one machine. Delay is
  faithful; contention is not: pinned nodes still share L3 cache and memory bandwidth. Close
  other work on the host and leave cores 0–39 to the cluster.
- **Closed loop.** Clients keep a fixed number of requests outstanding. That measures service
  latency without coordinated omission, but a closed loop cannot show behaviour past saturation
  that an open loop would (E2 sweeps the outstanding count to saturation instead).
- **Simulated work.** `spin` holds a core but has no cache or memory behaviour, so the absolute
  cost of real application work is not represented. E3 is the check on how much that matters.
- **Client capacity.** One client container (8 cores) generates all load; at the top of E2 check
  that it, rather than the replicas, is not the bottleneck (`OS_CPU_USER` of node 1000).
- **Per-client windows.** The five logical clients bootstrap sequentially, so their measurement
  windows are equal in length but not aligned in time.
- **Retries.** A failed run is retried (default twice) and runs later than its block, so is less
  well paired. `attempt` is recorded; analysis uses the last attempt.
- **The dirty tree.** Every `meta.json` records each repository's revision and its count of
  uncommitted files. Commit before a run that is going into a paper.

---

# Part 2: recovery from backtracks

A speculative executor runs batches before consensus has decided them. If consensus then decides
something different for a slot it already speculated (a view change, a re-proposal), everything
speculated from that slot on is wrong and must be undone. The three executors do it differently:

| | Mechanism | Cost should depend on |
|---|---|---|
| `dual_state` | Ask the confirmed worker for a copy of the confirmed state, replace the speculative state with it, **re-execute every kept batch** | state size (full clone) and *kept batches × batch size × processing cost* |
| `crud_single` / `crud_scalable` | Discard the pending deltas from the backtrack point, **re-merge the surviving deltas** into the accumulated cache | pending delta entries only; no clone, no re-execution of kept batches |

So the hypothesis is that `dual_state` recovery grows with the state and the CRUD executors' does
not (exponent ≈ 1 against ≈ 0), and that `dual_state` additionally pays processing cost for every
kept batch. R1–R4 test that; R5 looks at the related cost of taking a snapshot.

## How it is measured

`recovery_bench` drives each executor through its public handle with the same message sequence,
using the real crud_perf application and `State` type (so the clone is the real clone) and the
production replier. For one trial:

```
 speculate batches b … b+D-1                         D = speculation depth
 confirm b, wait for its reply                       barrier: queue is FIFO, so all D are speculated
 confirm the next C in a burst, don't wait           C = confirmed backlog (dual_state's confirmed
                                                         worker is still re-executing them)
 ── t0 ── re-propose slot k with different content   k = head (oldest unconfirmed) or tail (newest)
 confirm up to k
 ── t1 ── the corrected reply for k reaches the node        recovery time = t1 - t0
```

Time is taken **from outside** (re-proposal sent → corrected reply arrives at the fake network
node), so no executor is favoured by where an internal timer sits, and it is what a client sees.
`head` keeps nothing, isolating the state-dependent cost; `tail` keeps everything older, adding
the cost of re-executing (dual_state) or re-merging (CRUD) kept batches.

### Correctness is checked, not assumed

Every reply is compared with a reference model run sequentially on a plain map. A recovery that
is fast because it returned wrong answers is not a result: such a trial is reported as `WRONG`
and never averaged in. An executor that stops answering is reported as `wedged`, replaced with a
fresh one, and counted, so "how often does recovery fail" is a measured quantity. After each trial
the benchmark also checks that no reply arrives for speculation that should have been discarded.

## Experiments

| Plan | Varies | Question |
|---|---|---|
| `r1_state_size` | keys {10³ … 10⁶} × depth {8, 32, 128} × head/tail × cost {0, 20 µs} | does recovery scale with state size? with kept batches × cost? |
| `r2_confirmed_backlog` | confirmed batches in flight {0, 4, 16} × wait before the backtrack {0 … 50 ms} | does recovery survive a backtrack that arrives with confirmations in flight? |
| `r3_value_size` | value size {16 B … 4 KiB} at 10⁵ keys | bytes or entries? |
| `r4_batch_size` | ops per batch {10 … 2000} | how does it scale with batch size? |
| `r5_snapshot` | keys {10³ … 10⁶} | cost of a checkpoint, and does it stall speculation? |
| `r6_primitives` | keys {10³ … 10⁷}, pending batches {8 … 512} | how long do `State::clone` and the cache re-merge take on their own? |

R6 is the direct answer to "how long does rebuilding a cached state take", and the decomposition of
R1: it times the two operations outside any executor. `State::clone` is the real type; the re-merge
is a verbatim copy of the (crate-private) algorithm, so R1 stays the authoritative number.

Run the benchmark pinned (`cpus:` in the plan, default cores 40–47, clear of a cluster on 0–39)
so executor threads neither migrate nor compete with a running cluster.

## Reading the results

Medians with the 10th–90th percentile spread, not means with confidence intervals: trials of one
configuration run back to back on one executor are not independent replicates, and a t-interval
would claim precision they do not have. The first two trials of each configuration are discarded
as cold. The scaling table gives the slope of log(median recovery) on log(state bytes), over all
sizes and over the large ones only, because a fixed per-recovery floor flattens the small end of
any curve.

## Limitations

- **In-process, no network.** This isolates executor mechanics. It says nothing about when a
  backtrack happens or how often; for that see the view-change tests.
- **`--reproposal one`** re-proposes only the backtracked slot (isolating recovery);
  `all` also re-proposes every later slot, as a view change would.
- **Wedge restarts.** After a wedge the executor is replaced, so later trials run on a fresh one.
- **`op_cost_us`** is charged by spinning, so it needs real cores; hence the pinning.
- **Memory layout flatters `dual_state`.** `State::clone` costs 2–8× less when the source was
  itself produced by a clone (values allocated in hash-iteration order, so cloned sequentially)
  than when it was built by insertion: 1M keys, 80 ms against 174 ms; 10M keys, 684 ms against
  1,668 ms (R6 reports both). `dual_state`'s confirmed state is a clone-of-a-clone here (the
  executor hands its confirmed worker `state.clone()` at start-up), so its recovery and snapshot
  times are the optimistic end; a long-running state that has absorbed many writes drifts towards
  the slower layout. Treat R1's `dual_state` column as a lower bound.
- **The reply path is shared.** Every executor's replies go through the same replier and recorder
  node, ~µs per reply. At 2,000 operations per batch with many kept batches that is tens of
  milliseconds common to all three, which is a floor under the CRUD executors' figures in R4.

---

# Defects found while building this

Writing a *fair* comparison meant checking the harness for places it was not. Details and
reproductions are in [`FINDINGS.md`](FINDINGS.md); in short:

| | Status |
|---|---|
| No environment variable ever reached `BenchmarkConfig` (`Case::Upper` produced the key `"EXPENSIVE OP SLEEP MS"`), so every documented workload knob silently kept its default | **fixed**, with a test |
| Command-line knob overrides never reached the containers | **fixed** (compose passthrough) |
| Mixed/adversarial workloads drew from an unseeded RNG: no two runs saw the same requests | **fixed**, with a test |
| `crud_scalable` backtracks were invisible to metrics (no counter, rebuild untimed) | **fixed** |
| `dual_state` executor dies if a backtrack arrives while confirmations are in flight | **fixed**, with a regression test; R2 re-run 960 / 960 ok |
| Start-up hang (5 of 14 runs): clients started before the leader had authenticated them, and never retransmit | **fixed** (14 runs: 5 hung; after: 20 runs, 0); retransmission is still missing |
