# The Atlas bench metrics stack

InfluxDB (where every node writes) and Grafana (where you read it) as one Compose
project, joined to the benchmark's `atlas_network`.

You do not normally start it: every target that starts a run — `local`,
`remote-docker`, `remote-bare` — brings it up first and waits for InfluxDB to answer.
These drive it by hand:

```bash
# From testing-grounds/ — no project name needed
make metrics            # start; prints the URL. Idempotent, so re-run freely
make stop-metrics       # stop, keep both data volumes
make logs-metrics
make clean-metrics      # stop and drop the volumes — every measurement goes too
```

`grafana`, `stop-grafana`, `logs-grafana` and `clean-grafana` still work as aliases,
from when the stack was Grafana alone.

Grafana defaults to <http://127.0.0.1:3000> with anonymous access, so there is nothing
to log into. Dashboards live in the **Atlas** folder. InfluxDB publishes 8086 on
loopback only — containers on `atlas_network` reach it by its `influxdb` alias and do
not need the published port at all.

## Why InfluxDB lives here and not in the bench stack

It used to be emitted into the generated benchmark compose by
`gen-local-compose.sh`, behind `LOCAL_INFLUXDB=1`. That put the database on the run's
lifecycle: `make <project> local` tears its stack down when the run ends, so the server
disappeared at the exact moment you wanted to read what it had collected. (The data
survived in the volume; the thing that could serve it did not.) With the default
`LOCAL_INFLUXDB=0` there was no database at all — the checked-in `influx_db.toml`
names an instance that has to be running already.

Moving it here puts both halves of the observability stack on the same lifecycle,
which is the one that has to outlive the run.

## How the three parties agree

[`../scripts/gen-grafana.sh`](../scripts/gen-grafana.sh) runs before the stack comes
up and derives everything from one file,
[`../config-base/common/influx_db.toml`](../config-base/common/influx_db.toml) — the
same file the replicas and clients read:

| Generated | Consumed by | Carries |
|---|---|---|
| `../generated/grafana/influxdb.env` | the `influxdb` container | `INFLUXDB_DB`, admin user/password, auth flag |
| `../generated/grafana/grafana.env` | the `grafana` container | `INFLUX_URL`, `INFLUX_DB`, `INFLUX_USER`, `INFLUX_PASSWORD` |
| `../generated/grafana/datasources/influxdb.yml` | Grafana provisioning | the datasource, `uid: atlas-influx` |

and `LOCAL_INFLUXDB` selects the address:

| `LOCAL_INFLUXDB` | The database is | Nodes and Grafana reach it via |
|---|---|---|
| `1` (default) | the stack's own `influxdb` container | the `influxdb` alias on `atlas_network` (`http://influxdb:8086`) |
| `0` | an external instance | `ip` from `influx_db.toml`, ordinary routing |

There is deliberately no second copy of the connection details: change
`influx_db.toml` (or flip `LOCAL_INFLUXDB`) and re-run, and the writers, the server and
Grafana all move together. Only the *address* is ever overridden — the database name,
user and password always come from the TOML, which is why the server creates exactly
the database the nodes write to. `GRAFANA_INFLUX_*` in `bench.env` can override the
resolved values for Grafana alone, when you want it pointed somewhere else — reading
last week's results off another host, say.

Credentials reach Grafana through `grafana.env` and are referenced from the datasource
as `$INFLUX_PASSWORD` rather than inlined. That is not just tidiness: Grafana expands
`$VAR` inside provisioning files, and the checked-in password contains a `$`, so an
inlined copy would silently arrive truncated.

## Why the run waits for it

Atlas' OS monitor thread writes every 250 ms through
`rt::block_on(client.query(readings)).expect("Failed to write metrics to influxdb")`
(`Atlas/Atlas-Metrics/src/metrics/os_mon.rs`), and the workspace release profile sets
`panic = "abort"` — the profile the bench Dockerfile and `make build-binary` both use.
A node that starts before InfluxDB is listening therefore *aborts*; it does not run on
without OS metrics.

So [`../scripts/metrics-up.sh`](../scripts/metrics-up.sh) polls `SHOW DATABASES` until
the server answers and fails the run if it never does, rather than letting four
replicas start and die. It also issues `CREATE DATABASE`, which the image's own
`INFLUXDB_DB` cannot cover: that only runs on a first initialisation, so a volume
predating a `db_name` change would otherwise have no matching database and every write
would 404.

`METRICS_AUTOSTART=0` opts out of the automatic start (and says what you are taking
on); `INFLUXDB_RETENTION` bounds the default retention policy, which is worth setting
on a laptop — a busy suite writes tens of thousands of points a second.

## Networking

Both stacks declare `atlas_network` as `external: true`; the Makefile's
`ensure-network` target is the only thing that creates it. Consequences worth knowing:

* **Order does not matter.** The stack can start before or after a run.
* **It survives a run.** Tearing down the bench stack tries to remove the network,
  fails while the metrics stack is attached, and moves on — which is what you want,
  since you read the numbers after the run finishes.
* **WAN mode.** `WAN_ENABLED=1` needs the network to carry `WAN_SUBNET`. If the
  network already exists with a different subnet it has to be recreated, and that
  cannot happen while this stack holds it — `make stop-metrics` first. InfluxDB and
  Grafana both take dynamic addresses from the low end of the subnet, well clear of
  the static offsets (replicas .10+, clients .100+), and are left unshaped: they match
  no `tc` filter and so fall into the default class. Metrics traffic does not consume
  emulated WAN bandwidth.
* **Remote deployments.** `remote-docker` and `remote-bare` nodes read the unmodified
  `influx_db.toml` and cannot resolve the `influxdb` alias. To write into this host's
  database they need `influx_db.toml` pointed at this host and the port published
  beyond loopback: `INFLUXDB_BIND=0.0.0.0` (and then `INFLUXDB_AUTH_ENABLED=true`,
  since the default has HTTP auth off). The run targets print this reminder.

## Dashboards

One dashboard per test suite, plus a cross-suite overview. All land in the **Atlas**
folder.

| Dashboard | `make` target | Ordering protocol | Executor |
|---|---|---|---|
| Cluster Overview | — | any | any |
| microbenchmarks-async | `make microbenchmarks-async local` | febft PBFT | standard |
| app-scaling-tests | `make app-scaling-tests local` | febft PBFT | standard |
| preemptive_execution | `make preemptive_execution local` | febft PBFT | preemptive |
| microbenchmarks (legacy) | `make microbenchmarks local` | febft PBFT | none registered |
| crud_perf | `make crud_perf local` | febft PBFT | `EXECUTOR_VARIANT` |
| microbenchmark-hotstuff | `make microbenchmark-hotstuff local` | HotStuff (four-phase) | standard |
| microbenchmark-chainedhotstuff | `make microbenchmark-chainedhotstuff local` | Chained HotStuff | standard |

Each suite dashboard is laid out the same way: headline stats and an **About this
suite** note, then the **ordering protocol** rows, then the **executor** rows, then
the shared **Atlas baseline** — client-observed behaviour, request pre-processing,
replica loop, communication, logging/checkpointing/transfer, host resources. The
baseline is identical across suites on purpose: it is what makes two suites
comparable panel-for-panel.

Two template variables filter everything: **Node** (`host` tag, `NodeId(n)` — replicas
`0..n-1`, clients from `1000`) and **Run** (`extra` tag, `None` unless the run set
`INFLUX_EXTRA`).

### Why panels differ between suites

A metric reaches InfluxDB only if the suite's binary passes its crate's `metrics()` to
`initialize_metrics` **and** the metric's own level survives that binary's
`with_metric_level(..)`. Levels order `Disabled < Trace < Debug < Info`, and a metric
is kept when its level is **at or above** the configured one — so the default `Info`
is the *most* restrictive setting, not the least.

| Suite | Replica level | Client level | Emitted | Filtered out |
|---|---|---|---|---|
| microbenchmarks-async | `Debug` | `Info` | 89 | 16 |
| app-scaling-tests | `Trace` | `Trace` | 99 | 2 |
| preemptive_execution | `Debug` | `Info` | 107 | 16 |
| microbenchmarks (legacy) | `Trace` | `Info` | 75 | 3 |
| crud_perf | `Debug` (+ tracking) | `Info` | 119 | 14 |
| microbenchmark-hotstuff | `Debug` | `Info` | 73 | 16 |
| microbenchmark-chainedhotstuff | `Debug` | `Info` | 73 | 16 |

Consequences worth knowing before you conclude something is broken:

* **All four `atlas-comm-mio` metrics are Trace**, and the two suites that run at Trace
  do not register that crate — so `MESSAGES_IN_CHANNEL`, `MESSAGE_DISPATCH_TIME`,
  `MESSAGE_WAKER_TIME` and `REQUEST_SEND_TIME` are emitted by no suite at all today.
* **Queue depths are Trace.** `REPLICA_RQ_QUEUE_SIZE` and
  `DECISION_LOG_WORK_QUEUE_SIZE` only ever appear in app-scaling-tests and
  microbenchmarks-legacy. If you are chasing a bottleneck elsewhere, raise that
  binary's level.
* **The preemptive executors emit `OPERATIONS_EXECUTED_PER_SECOND` under the same name
  as the standard one.** `atlas-smr-preemptive-execution` registers it (and its unordered
  twin) at IDs 824/825, counted on the confirmation path, so a crud_perf run built with
  `crud_single`, `crud_scalable` or `dual_state` fills the same throughput panel as
  `baseline` and as every other suite. Before that it was blank for three of the four
  variants, and preemptive_execution's headline stat fell back to `OPERATIONS_ORDERED`.
* **`RQ_CLIENT_TRACKING` / `RQ_BATCH_TRACKING` are registered `Disabled`.** crud_perf
  is the only suite that overrides them, which is why per-request tracking exists there
  and nowhere else.
* **microbenchmarks (legacy) registers neither `atlas-smr-core` nor
  `atlas-smr-execution`**, so it has no `CONSENSUS_WAIT_TIME`, no request
  pre-processing row and no `OPERATIONS_EXECUTED_PER_SECOND`. Its throughput comes off
  `OPERATIONS_ORDERED` instead, and its dashboard's headline stat says so.

Each dashboard's **About this suite** panel lists that suite's filtered-out metrics by
name and level, so the blank is explained in place rather than left to guess at.

### Protocol-specific rows

The three protocols measure genuinely different things, so they get different rows
rather than a shared "consensus" row with holes in it:

* **febft PBFT** — proposer and batching, the pre-prepare → prepare → commit ladder
  (the two `*_TO_*` transitions are the quorum round trips, and where WAN delay lands),
  then synchronisation/view change.
* **HotStuff, four-phase** — the prepare → pre-commit → commit → decide ladder plus
  `SIGNATURE_*`: each phase is one leader collecting a quorum certificate, so threshold
  signing sits on the critical path in a way it does not for PBFT's all-to-all rounds.
* **Chained HotStuff** — no ladder; one pipelined generic phase, measured as the
  vote/proposal round trip (`VOTE_SEND` → `VOTE_RECEIVED` → `PROPOSAL_SEND`) plus
  `END_TO_END_LATENCY`, which spans several pipelined heights and is legitimately much
  larger than their sum.

Two traps this layout works around:

1. `hot_iron_oxide::metric::metrics()` registers the four-phase **and** chained metrics
   whichever protocol you run, so half of them are registered but never recorded. Each
   HotStuff dashboard draws only its own protocol's row; an absent row is correct.
2. `PREPARE_LATENCY` and `COMMIT_LATENCY` exist in **both** febft and hot-iron-oxide
   with different meanings, and every suite writes to the same database. Tag runs with
   `INFLUX_EXTRA` and use the **Run** filter before comparing across protocols.

### No dashboard for these

* **correctness-testing** is a `[lib]` with no binary, run in-process under
  `cargo nextest`; it never calls `initialize_metrics` and writes nothing to InfluxDB.
  (Its `bench/bench.env` names a `correctness_testing` binary that does not exist, so
  `make correctness-testing local` cannot build either.)
* **IronDumbo** has no metrics instrumentation at all — no `MetricKind` anywhere in the
  crate — and no suite depends on it. Giving it a dashboard means first registering
  metrics for RBC/ABA/DumboBFT, at which point add an entry to `SUITES` in the
  generator.

## Regenerating

The dashboards are generated, not hand-written:

```bash
python3 bench/grafana/build-dashboards.py
```

`build-dashboards.py` parses the metric registries straight out of the Rust sources on
every run and filters each suite's panels against what that suite can actually emit, so
a renamed metric or a changed level is picked up rather than silently producing an
empty panel. The suite → crate → level composition is the one part it cannot infer: it
mirrors each binary's `initialize_metrics` call and lives in the `SUITES` table at the
top of the file. Update it when a suite gains or drops a crate.

`allowUiUpdates` is on, so you can tweak a panel in Grafana to explore — but land the
change in the generator, because a regeneration overwrites `dashboards/*.json`.

### Reading the numbers

Atlas' metrics thread collects and *resets* every metric once a second, which fixes how
each kind should be read:

| Metric kind | Stored as | Panel does |
|---|---|---|
| `Counter` | count during that 1s window | reads as a per-second rate already — no `derivative()` |
| `Duration` | mean nanoseconds, plus a `std_dev` field | `mean("value")`, unit `ns` |
| `Count` | mean of the values recorded (batch sizes, queue depths) | `mean("value")` — never a sum |
| `CountMax` | maximum seen in the window | max, not an average |
| `CorrelationDurationTracker` | one point **per request**, `correlation_id` as a field | real percentiles — `percentile("value", 99)` |
| `CorrelationAggrDurationTracker` | mean over tracked requests | `mean("value")` |
| `Correlation` | one point per event, stage in the `event` tag | `count("correlation_id") GROUP BY "event"` |

The correlation kinds only carry data in crud_perf, and they are the only place a
latency *distribution* exists — everything else has already been averaged into a
one-second bucket before it reaches InfluxDB.

Two measurement quirks are baked into the panel descriptions and worth repeating:

* **`OS_CPU_USER` is a 0–1 fraction per core** (tag `cpu`), sampled every 250 ms; the
  panels average over cores and use `percentunit`.
* **`OS_NETWORK_UP` / `OS_NETWORK_DOWN` are crossed** in
  `Atlas/Atlas-Metrics/src/metrics/os_mon.rs` — `UP` is summed from `received()` and
  `DOWN` from `transmitted()`. The panels plot both and say so; trust the series names
  over the words.
