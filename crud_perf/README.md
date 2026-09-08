# Preemptive execution benchmark

Compares Atlas's speculative (preemptive) executors against the standard post-commit
executor, focusing on the latency the feature is meant to remove: time spent waiting for
consensus before execution can start.

## The four variants

One binary, one executor selected at compile time (Rust type aliases are compile-time, so
this cannot be a runtime switch). Everything except the executor itself is shared between
variants, so they cannot silently drift apart and invalidate the comparison.

Each variant lives in its own module under `src/executor_variant/`, contributing just three
items -- the `Executor` type, a `NAME` label, and its `metrics()` registration. The module
docs there explain what each executor actually does and what to watch when benchmarking it.
`src/executor_variant/mod.rs` is the only file in the crate that mentions the feature flags.

| `EXECUTOR_VARIANT` | Module | Executor | Notes |
|---|---|---|---|
| `baseline` | `baseline.rs` | `SingleThreadedMonExecutor` | `atlas-smr-execution`; executes after commit. The comparison baseline. |
| `dual_state` | `dual_state.rs` | `MonolithicPreemptiveExecutor` | Two full state copies; confirmed worker re-executes every batch. |
| `crud_single` | `crud_single.rs` | `CRUDMonolithicPreemptiveExecutor` | Single-threaded cache/delta; applies a precomputed delta on confirm. Default. |
| `crud_scalable` | `crud_scalable.rs` | `ScalableCRUDMonolithicPreemptiveExecutor` | As above, but parallel within a batch with collision detection. |

Set it in `bench/bench.env`, or override per run. Every mode that *builds* honours it:

```bash
make crud_perf local        EXECUTOR_VARIANT=baseline   # Docker; variant is a build arg
make crud_perf build-binary EXECUTOR_VARIANT=baseline   # native binary (also remote-bare)
make crud_perf remote-bare  EXECUTOR_VARIANT=baseline
```

`local` bakes the variant into the image and into its tag (`crud-perf-baseline`,
`crud-perf-crud-single`, ...), so switching variants cannot silently reuse the previous
build's image.

`remote-docker` is the exception: it builds nothing and every machine pulls
`DOCKER_IMAGE:DOCKER_VERSION`, so the variant has to be baked in when that image is built
and pushed, and distinguished by its tag. The target prints the exact build/push/deploy
commands when it detects a variant it cannot honour. See
`../bench/README.md#compile-time-variants-executor_variant`.

Building directly (note: Cargo features are additive and `crud_single` is the default, so a
non-default variant needs `--no-default-features`):

```bash
cargo build --release --no-default-features --features crud_scalable
```

Selecting zero or more than one variant is a compile error, not a silent misconfiguration.

Confirm what was actually built rather than what you asked for — each replica logs
`executor_variant=<name>` at startup, and the same name is stamped as the InfluxDB `extra`
tag whenever `INFLUX_EXTRA` is unset:

```bash
docker exec atlas-influxdb influx -database atlas \
    -execute 'SHOW TAG VALUES WITH KEY = "extra"'
```

## What gets measured

The metric the feature exists to move is **`CONSENSUS_WAIT_TIME`**: the interval from the
ordering protocol first knowing a batch's requests (pre-prepare complete) to that batch
reaching the executor. It is stamped in `febft`'s `finalize_pre_prepare` and recorded in
`SMRExecWrapper::transform_update_batch` — one call site every executor funnels through, so
baseline and preemptive report it identically and comparably.

Expect it to be **near-zero for the preemptive variants** (they start on the proposal) and
roughly **a consensus round trip for the baseline** (it waits for the commit). If the two
match, the preemptive path is not actually engaging — see *Verifying speculation is live*.

Batches replayed from a persisted proof during log transfer are deliberately left unstamped:
they never waited on consensus, so they contribute no sample.

End-to-end latency needs no new instrumentation — both systems share the reply path, so
`CLIENT_RQ_LATENCY` (client-observed) and `RQ_CLIENT_TRACK_GLOBAL` (server-side, per-request)
already cover both. The per-request correlation trackers are registered `Disabled` upstream
and are re-enabled here in `replica.rs::enable_request_tracking`.

Beyond latency, the comparison is only meaningful alongside the *cost* of speculation:
backtrack counts, collision rate, pending-queue depth, confirm-path cost, and CPU/RAM. A
latency win at a 30% backtrack rate is a different result from one at 0.1%.

## Running and analysing

Each run tags its metrics with the compiled-in variant (via InfluxDB's `extra` tag), set
automatically unless `INFLUX_EXTRA` is exported. Run each variant, then:

```bash
./analysis/compare_variants.py --url http://localhost:8086 --db atlas --since 1h
```

`atlas-metrics` stores rolling mean/stddev (Welford), not histograms — there are no
percentiles. Latency claims from this harness are mean ± stddev only.

## Verifying speculation is live

The preemptive path is selected by Rust specialization, which fails *silently*: if
`SMRExecWrapper` stops satisfying `TPreemptiveExecutorDecisionHandle` /
`TPreemptiveExecutorStateHandle`, the replica quietly falls back to the post-commit path and
every variant still builds, runs, and produces plausible numbers — while measuring the
baseline against itself.

Two guards:

- `atlas-smr-preemptive-execution`'s `exec_handle.rs::specialization_guard` turns that
  regression into a compile error.
- At runtime, confirm the speculative metrics are non-zero: `CACHE_PREEMPTIVE_EXECUTION_TIME`
  (crud_single), `DS_PREEMPTIVE_EXECUTION_TIME` (dual_state), or
  `SCALABLE_PREEMPTIVE_EXECUTION_TIME` (crud_scalable).

## Known gap: dual-state and state transfer

`single_thread_double_state/preemptive_worker/mod.rs` still has a `todo!()` on the path that
handles a message from the confirmed worker while in state-transfer mode. It is not on the
steady-state execution path, but a `dual_state` run that triggers state transfer (replica
crash-and-rejoin, log watermark overflow, partition-induced CST) can panic. Keep steady-state
latency runs free of forced state transfer; fixing it is a prerequisite only for benchmarking
preemptive execution under faults.

## Workload knobs

See `bench/bench.env` — key space (drives collision/backtrack rate), key distribution
(uniform/zipf), CRUD operation mix, injected per-op cost, and forced-ordered mode. Backtrack
and collision rates are workload-dependent, so a single configuration proves little; sweep
request rate, batch size, client count, and contention.
