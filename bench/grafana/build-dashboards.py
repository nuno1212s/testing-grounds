#!/usr/bin/env python3
"""Generate the Atlas bench Grafana dashboards.

    python3 bench/grafana/build-dashboards.py            # writes bench/grafana/dashboards/

Why a generator rather than eight hand-written JSON files: a panel is only worth
drawing if the suite it belongs to actually emits that measurement, and *that* is
decided in Rust, in two places at once —

  1. which crates' `metrics()` a suite's binary passes to `initialize_metrics`, and
  2. `with_metric_level(..)`, which drops every metric registered below it.

So this script parses the metric registries out of the Rust sources on every run and
filters each suite's panels against them. A metric that moves, is renamed, or changes
level is picked up the next time you run this; a panel that could never have data is
not drawn at all, and is instead listed in the dashboard's own "not emitted here" note.

Editing: tweak a panel in Grafana to explore, but land the change here — a regeneration
overwrites dashboards/*.json.
"""
import json
import os
import re
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
OUT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "dashboards")

DS = {"type": "influxdb", "uid": "atlas-influx"}

# ── metric registry, parsed from the Rust sources ────────────────────────────────
# Every crate whose `metrics()` any suite registers. Paths are relative to the
# workspace root (the directory holding Atlas/, febft/, hot-iron-oxide/, ...).
REGISTRY_FILES = {
    "atlas-client":         "Atlas/Atlas-Client/src/metric/mod.rs",
    "atlas-comm-mio":       "Atlas/Atlas-Comm-MIO/src/metrics/mod.rs",
    "atlas-communication":  "Atlas/Atlas-Communication/src/metric/mod.rs",
    "atlas-core":           "Atlas/Atlas-Core/src/metric/mod.rs",
    "atlas-decision-log":   "Atlas/Atlas-Decision-Log/src/metric/mod.rs",
    "atlas-log-transfer":   "Atlas/Atlas-Log-Transfer/src/metrics/mod.rs",
    "atlas-smr-core":       "Atlas/Atlas-SMR-Core/src/metric/mod.rs",
    "atlas-smr-execution":  "Atlas/Atlas-SMR-Execution/src/metric/mod.rs",
    "atlas-smr-preemptive": "Atlas/Atlas-SMR-Preemptive-Execution/src/metric.rs",
    "atlas-smr-replica":    "Atlas/Atlas-SMR-Replica/src/metric/mod.rs",
    "atlas-view-transfer":  "Atlas/Atlas-View-Transfer/src/metrics/mod.rs",
    "febft-pbft":           "febft/febft-pbft-consensus/src/bft/metric/mod.rs",
    "febft-state-transfer": "febft/febft-state-transfer/src/metrics/mod.rs",
    "hotstuff-shared":      "hot-iron-oxide/src/metric.rs",
    "hotstuff-basic":       "hot-iron-oxide/src/hot_iron/metric.rs",
    "hotstuff-chained":     "hot-iron-oxide/src/chained/metrics.rs",
    "crud-app":             "testing-grounds/crud_perf/crud_perf_exec/src/metric/mod.rs",
    # OS_* are written by a second thread in Atlas-Metrics and bypass the registry
    # and the level filter entirely — they are always present.
}

# (ID, NAME.to_string(), MetricKind::Kind [, MetricLevel::Level])
_ENTRY = re.compile(
    r"\(\s*[A-Za-z_0-9:]*?[A-Z_0-9]+_ID\s*,\s*[A-Za-z_0-9:]*?([A-Z_0-9]+)\.to_string\(\)\s*,"
    r"\s*(?:atlas_metrics::metrics::)?MetricKind::(\w+)"
    r"(?:\s*,\s*(?:atlas_metrics::)?MetricLevel::(\w+))?")

# Disabled < Trace < Debug < Info, and a metric is emitted when its own level is
# >= the level the binary configured (Atlas-Metrics/src/metrics/mod.rs).
LEVEL_ORD = {"Disabled": 0, "Trace": 1, "Debug": 2, "Info": 3}


def parse_registry():
    reg = {}
    for crate, rel in REGISTRY_FILES.items():
        path = os.path.join(ROOT, rel)
        if not os.path.exists(path):
            sys.exit(f"ERROR: {rel} not found under {ROOT}.\n"
                     f"       The metric registry moved; update REGISTRY_FILES.")
        src = open(path).read()
        names = dict(re.findall(r'const\s+([A-Z_0-9]+)\s*:\s*&str\s*=\s*"([^"]+)"', src))
        entries = {}
        for m in _ENTRY.finditer(src):
            entries[names.get(m.group(1), m.group(1))] = (m.group(2), m.group(3) or "Info")
        if not entries:
            sys.exit(f"ERROR: parsed no metrics out of {rel}; the registry form changed.")
        reg[crate] = entries
    return reg


REG = parse_registry()

# Written directly by Atlas-Metrics' OS monitor thread, not through the registry.
OS_METRICS = {"OS_CPU_USER": ("Gauge", "Info"), "OS_RAM_USAGE": ("Gauge", "Info"),
              "OS_NETWORK_UP": ("Gauge", "Info"), "OS_NETWORK_DOWN": ("Gauge", "Info")}


def emitted(crates, level):
    """Measurements a role emits: registered by one of `crates`, at or above `level`."""
    floor = LEVEL_ORD[level]
    out = {}
    for crate in crates:
        for name, (kind, lvl) in REG[crate].items():
            if LEVEL_ORD[lvl] >= floor:
                out[name] = (kind, lvl)
    return out


def suppressed(crates, level):
    """Registered by this suite but filtered out by its level — the honest blank list."""
    floor = LEVEL_ORD[level]
    out = {}
    for crate in crates:
        for name, (kind, lvl) in REG[crate].items():
            if LEVEL_ORD[lvl] < floor:
                out[name] = lvl
    return out


# ── suite composition ─────────────────────────────────────────────────────────────
# Mirrors each binary's initialize_metrics call. Keep in step with the Rust when a
# suite gains or drops a crate; the generator cannot infer this part.
ATLAS_REPLICA_CORE = ["atlas-core", "atlas-communication", "atlas-smr-replica",
                      "atlas-smr-core", "atlas-log-transfer", "febft-state-transfer",
                      "atlas-view-transfer"]

SUITES = {
    "microbenchmarks-async": dict(
        uid="atlas-suite-microbench-async",
        title="Atlas — microbenchmarks-async",
        make="make microbenchmarks-async local",
        protocol="pbft", executor="standard",
        blurb=(
            "Null application: requests, replies and state are fixed-size opaque blobs "
            "(REQUEST_SIZE / REPLY_SIZE / STATE_SIZE), so the application does no work "
            "and what is left on the clock is the framework itself. Read EXECUTION_TIME_TAKEN "
            "as the floor, and treat anything large in the pipeline rows as Atlas overhead "
            "rather than app cost."),
        replica_crates=["febft-pbft"] + ATLAS_REPLICA_CORE + ["atlas-smr-execution", "atlas-comm-mio"],
        replica_level="Debug",
        client_crates=["atlas-communication", "atlas-core", "atlas-comm-mio", "atlas-client"],
        client_level="Info",
    ),
    "app-scaling-tests": dict(
        uid="atlas-suite-app-scaling",
        title="Atlas — app-scaling-tests",
        make="make app-scaling-tests local",
        protocol="pbft", executor="standard",
        blurb=(
            "A real keyed store — Read/Write/Delete over a Zipf-distributed 128k-key space — "
            "so unlike microbenchmarks-async the application itself costs something. Execution "
            "time and state-digest/checkpoint cost are the point here; watch them against "
            "throughput as the state grows.\n\n"
            "This is the one suite that runs BOTH roles at MetricLevel::Trace, so it is the "
            "only place the Trace-level replica-loop and queue-depth metrics ever appear — at "
            "the cost of noticeably more metric traffic."),
        replica_crates=["febft-pbft"] + ATLAS_REPLICA_CORE + ["atlas-smr-execution"],
        replica_level="Trace",
        client_crates=["atlas-communication", "atlas-client"],
        client_level="Trace",
    ),
    "preemptive-execution": dict(
        uid="atlas-suite-preemptive",
        title="Atlas — preemptive_execution",
        make="make preemptive_execution local",
        protocol="pbft", executor="preemptive",
        blurb=(
            "PBFT ordering with the speculative executor: work starts on the proposal instead "
            "of waiting for the commit. The bet is that CONSENSUS_WAIT_TIME — dead time in the "
            "baseline — gets spent usefully; the cost is backtracks when the committed order "
            "differs from the proposed one. Judge the suite on those two together, never on "
            "throughput alone."),
        replica_crates=["febft-pbft"] + ATLAS_REPLICA_CORE + ["atlas-smr-preemptive", "atlas-comm-mio"],
        replica_level="Debug",
        client_crates=["atlas-communication", "atlas-core", "atlas-comm-mio", "atlas-client"],
        client_level="Info",
    ),
    "microbenchmarks-legacy": dict(
        uid="atlas-suite-microbench-legacy",
        title="Atlas — microbenchmarks (legacy)",
        make="make microbenchmarks local",
        protocol="pbft", executor="none",
        blurb=(
            "The pre-SMR-Core benchmark, kept for comparison against the older API. It "
            "registers neither atlas-smr-core nor atlas-smr-execution, so the request "
            "pre-processing and execution rows other suites rely on simply do not exist here — "
            "there is no CONSENSUS_WAIT_TIME and no OPERATIONS_EXECUTED_PER_SECOND. Throughput "
            "has to be read off the ordering protocol (OPERATIONS_ORDERED) and the clients.\n\n"
            "Its replica runs at Trace, so what it does report, it reports in full."),
        replica_crates=["febft-pbft", "atlas-core", "atlas-communication", "atlas-smr-replica",
                        "atlas-log-transfer", "febft-state-transfer", "atlas-view-transfer"],
        replica_level="Trace",
        client_crates=["atlas-communication", "atlas-client"],
        client_level="Info",
    ),
    "crud-perf": dict(
        uid="atlas-suite-crud-perf",
        title="Atlas — crud_perf",
        make="make crud_perf local",
        protocol="pbft", executor="both",
        blurb=(
            "The richest suite, and the only one with an instrumented application (CRUD_*) and "
            "per-request tracking switched on: it overrides RQ_CLIENT_TRACKING and "
            "RQ_BATCH_TRACKING from Disabled to Debug, so the correlation rows below have data "
            "here and nowhere else.\n\n"
            "EXECUTOR_VARIANT picks what the executor row shows: `baseline` registers "
            "atlas-smr-execution, while `crud_single`, `crud_scalable` and `dual_state` register "
            "atlas-smr-preemptive. Both rows are drawn; the one for the variant you did not build "
            "stays empty. Workload shape comes from KEY_DISTRIBUTION, READ/CREATE/UPDATE/DELETE_RATIO "
            "and WORKLOAD_TYPE."),
        replica_crates=["febft-pbft"] + ATLAS_REPLICA_CORE +
                       ["atlas-smr-execution", "atlas-smr-preemptive", "atlas-comm-mio", "crud-app"],
        replica_level="Debug",
        replica_level_overrides={"RQ_CLIENT_TRACKING": "Debug", "RQ_BATCH_TRACKING": "Debug"},
        client_crates=["atlas-communication", "atlas-core", "atlas-comm-mio", "atlas-client", "crud-app"],
        client_level="Info",
    ),
    "microbenchmark-hotstuff": dict(
        uid="atlas-suite-hotstuff",
        title="Atlas — microbenchmark-hotstuff",
        make="make microbenchmark-hotstuff local",
        protocol="hotstuff-basic", executor="standard",
        blurb=(
            "Four-phase HotStuff (hot-iron-oxide `src/hot_iron/`). The phase ladder — "
            "PREPARE → PRE_COMMIT → COMMIT → DECIDED → FINALIZED — is the protocol row below.\n\n"
            "Two traps. hot_iron_oxide::metric::metrics() registers the chained metrics too, so "
            "VOTE_SEND_LATENCY and friends are registered here but never recorded: an empty "
            "chained row is correct, not broken. And PREPARE_LATENCY / COMMIT_LATENCY are also "
            "febft metric names with different meanings — filter by Run before comparing a "
            "HotStuff series against a PBFT one in the same database."),
        replica_crates=["hotstuff-shared", "hotstuff-basic", "hotstuff-chained"] +
                       ATLAS_REPLICA_CORE + ["atlas-smr-execution", "atlas-comm-mio"],
        replica_level="Debug",
        client_crates=["atlas-communication", "atlas-core", "atlas-comm-mio", "atlas-client"],
        client_level="Info",
    ),
    "microbenchmark-chainedhotstuff": dict(
        uid="atlas-suite-chainedhotstuff",
        title="Atlas — microbenchmark-chainedhotstuff",
        make="make microbenchmark-chainedhotstuff local",
        protocol="hotstuff-chained", executor="standard",
        blurb=(
            "Chained HotStuff (hot-iron-oxide `src/chained/`): one pipelined generic phase "
            "instead of four, so there is no phase ladder — the protocol row is the vote/proposal "
            "round trip (VOTE_SEND → VOTE_RECEIVED → PROPOSAL_SEND) plus END_TO_END_LATENCY.\n\n"
            "The four-phase metrics are registered by the same crate and stay empty here; that is "
            "expected. Note the two roles measure different things: VOTE_SEND_LATENCY is recorded "
            "by every replica, VOTE_RECEIVED / PROPOSAL_SEND only by the leader of that height, so "
            "expect those series on one node at a time."),
        replica_crates=["hotstuff-shared", "hotstuff-basic", "hotstuff-chained"] +
                       ATLAS_REPLICA_CORE + ["atlas-smr-execution", "atlas-comm-mio"],
        replica_level="Debug",
        client_crates=["atlas-communication", "atlas-core", "atlas-comm-mio", "atlas-client"],
        client_level="Info",
    ),
}


# ── units ─────────────────────────────────────────────────────────────────────────
# Atlas records every Duration in nanoseconds; Grafana's "ns" unit rescales for display.
UNIT_OVERRIDE = {
    "OPERATIONS_EXECUTED_PER_SECOND": "ops",
    "UNORDERED_OPERATIONS_EXECUTED_PER_SECOND": "ops",
    "OPERATIONS_ORDERED": "ops",
    "CLIENT_RQ_PER_SECOND": "reqps",
    "CLIENT_RQ_RECV_PER_SECOND": "reqps",
    "CRUD_CLIENT_OPS_DONE": "ops",
    "OUTGOING_MESSAGE_SIZE": "bytes",
    "INCOMING_MESSAGE_SIZE": "bytes",
    "CACHE_DELTA_SIZE": "bytes",
    "OS_RAM_USAGE": "bytes",
    "OS_CPU_USER": "percentunit",
}
KIND_UNIT = {
    "Duration": "ns", "CorrelationDurationTracker": "ns",
    "CorrelationAggrDurationTracker": "ns",
    "Counter": "cps", "CounterCorrelation": "cps",
    "Count": "short", "CountMax": "short", "Correlation": "short", "Gauge": "short",
}


def unit_of(name, kind):
    return UNIT_OVERRIDE.get(name, KIND_UNIT.get(kind, "short"))


# ── query + panel construction ────────────────────────────────────────────────────
SEL = '("host" =~ /^$host$/ AND "extra" =~ /^$run$/) AND $timeFilter'


def q(measurement, expr='mean("value")', by_host=True):
    group = ', "host"' if by_host else ""
    return (f'SELECT {expr} FROM "{measurement}" WHERE {SEL} '
            f'GROUP BY time($__interval){group} fill(none)')


def target(query, alias, ref):
    t = {"refId": ref, "datasource": DS, "query": query, "rawQuery": True,
         "resultFormat": "time_series"}
    # An empty alias is not the same as no alias: the InfluxDB datasource applies it
    # literally and the series ends up nameless, which a stat panel renders as blank.
    if alias:
        t["alias"] = alias
    return t


_REFS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"


def series(E, specs):
    """specs: [(measurement, legend)] — drops any this suite cannot emit."""
    out = []
    for measurement, legend in specs:
        if measurement not in E:
            continue
        out.append(target(q(measurement), f"{legend} $tag_host", _REFS[len(out) % 26]))
    return out


class Layout:
    """Places panels left-to-right, wrapping at 24 columns."""

    def __init__(self):
        self.panels, self.y, self.x = [], 0, 0

    def row(self, title, description=""):
        if self.x:
            self.y += self.h
            self.x = 0
        self.panels.append({"type": "row", "title": title, "collapsed": False,
                            "panels": [], "gridPos": {"h": 1, "w": 24, "x": 0, "y": self.y},
                            "description": description})
        self.y += 1
        return self

    def add(self, panel, w=12, h=9):
        if panel is None:
            return self
        if self.x + w > 24:
            self.y += self.h
            self.x = 0
        panel["gridPos"] = {"h": h, "w": w, "x": self.x, "y": self.y}
        self.panels.append(panel)
        self.x, self.h = self.x + w, h
        return self

    h = 9


def graph(title, targets, unit, desc="", stack=False):
    if not targets:
        return None
    return {
        "type": "timeseries", "title": title, "description": desc, "datasource": DS,
        "targets": targets,
        "fieldConfig": {"defaults": {"unit": unit, "custom": {
            "drawStyle": "line", "lineInterpolation": "linear", "lineWidth": 1,
            "fillOpacity": 8, "showPoints": "never", "spanNulls": True,
            "axisSoftMin": 0,
            "stacking": {"mode": "normal" if stack else "none", "group": "A"}}},
            "overrides": []},
        "options": {"legend": {"displayMode": "table", "placement": "bottom",
                               "showLegend": True,
                               "calcs": ["mean", "max", "lastNotNull"]},
                    "tooltip": {"mode": "multi", "sort": "desc"}},
    }


def stat(title, query, unit, desc="", decimals=1, calc="mean"):
    return {
        "type": "stat", "title": title, "description": desc, "datasource": DS,
        "targets": [target(query, "", "A")],
        "fieldConfig": {"defaults": {"unit": unit, "decimals": decimals,
                                     "color": {"mode": "fixed", "fixedColor": "text"}},
                        "overrides": []},
        "options": {"reduceOptions": {"calcs": [calc], "fields": "", "values": False},
                    "orientation": "auto", "textMode": "auto", "colorMode": "none",
                    "graphMode": "area", "justifyMode": "auto"},
    }


def text_panel(title, markdown):
    # The built-in "grafana" datasource, not null: a null datasource leaves the panel
    # unrendered rather than falling back to the default.
    return {"type": "text", "title": title,
            "datasource": {"type": "datasource", "uid": "grafana"},
            "options": {"mode": "markdown", "content": markdown, "code": {
                "language": "plaintext", "showLineNumbers": False, "showMiniMap": False}},
            "transparent": False, "fieldConfig": {"defaults": {}, "overrides": []}}


# ── shared baseline: the Atlas components every suite is built out of ─────────────
def baseline_rows(L, E):
    L.row("Client-observed behaviour",
          "What the load generator sees. The only view that is not the system marking "
          "its own homework — if these and the replica-side numbers disagree, believe these.")
    L.add(graph("Client request rate", series(E, [
        ("CLIENT_RQ_PER_SECOND", "sent"),
        ("CLIENT_RQ_RECV_PER_SECOND", "replies received"),
    ]), "reqps",
        "One series per client container (NodeId 1000+), each aggregating N_CLIENTS "
        "logical clients. Counter metrics are reset every collection, so a point is "
        "already a per-second rate."))
    L.add(graph("Client request latency", series(E, [
        ("CLIENT_RQ_LATENCY", "ordered"),
        ("CLIENT_UNORDERED_RQ_LATENCY", "unordered"),
        ("CLIENT_RQ_DELIVER_RESPONSE", "deliver response"),
        ("CLIENT_RQ_SEND_TIME", "send"),
        ("CLIENT_RQ_RECV_TIME", "receive"),
    ]), "ns",
        "Ordered latency is send → f+1 matching replies. CLIENT_RQ_SEND_TIME is "
        "registered at Trace, so it only appears in suites whose client runs at that level."))
    L.add(graph("Client timeouts", series(E, [("CLIENT_RQ_TIMEOUT", "timeouts")]), "cps",
                "Non-zero means requests are not completing on the first attempt. "
                "Sustained timeouts on the replica side precede a view change."), w=24, h=6)

    L.row("Request pre-processing",
          "atlas-smr-core: the path from a client message arriving to a request being "
          "proposable. Where load sheds before consensus ever sees it.")
    L.add(graph("Stage durations", series(E, [
        ("RQ_PRE_PROCESSING_CLIENT_MSGS", "client msgs"),
        ("RQ_PRE_PROCESSING_WORKER_ORDERED_PROCESS", "worker ordered"),
        ("RQ_PRE_PROCESSING_WORKER_DECIDED_PROCESS_TIME", "worker decided"),
        ("RQ_PRE_PROCESSING_ORCHESTRATOR_WORKER_PASSING_TIME", "orchestrator→worker"),
        ("RQ_PRE_PROCESSING_WORKER_PROPOSER_PASSING_TIME", "worker→proposer"),
        ("RQ_PRE_PROCESSING_COLLECT_PENDING", "collect pending"),
        ("RQ_PRE_PROCESSING_CLONE_RQS", "clone rqs"),
        ("RQ_PRE_PROCESSING_FWD_RQS", "forwarded rqs"),
        ("RQ_PRE_PROCESSING_TIMEOUT_RQS", "timeout rqs"),
        ("RQ_PRE_PROCESSING_DECIDED_RQS", "decided rqs"),
        ("RQ_PRE_PROCESSING_WORKER_STOPPED_TIME", "worker stopped"),
        ("RQ_CLONE_PENDING_TIME", "clone pending (core)"),
        ("RQ_COLLECT_PENDING_TIME", "collect pending (core)"),
    ]), "ns"))
    L.add(graph("Pre-processing rates and batch size", (
        series(E, [
            ("RQ_PRE_PROCESSING_CLIENT_COUNT", "client rqs/s"),
            ("RQ_PRE_PROCESS_ORCHESTRATOR_MESSAGES_PROCESSED", "orchestrator msgs/s"),
            ("RQ_PRE_PROCESSING_WORKER_ORDERED_PROCESS_TIME", "worker ordered/s"),
            ("RQ_PRE_PROCESSING_DISCARDED_REQUESTS", "discarded/s"),
            ("RQ_PRE_PROCESSING_BATCH_SIZE", "batch size"),
        ])), "short",
        "Mixed units by design: the discard rate only means something next to the "
        "intake rate. RQ_PRE_PROCESSING_BATCH_SIZE is a Count — averaged, never summed."))

    L.row("Replica loop",
          "atlas-smr-replica: the main loop that polls the ordering protocol, feeds the "
          "decision log and drains timeouts. Several of these are Trace-only.")
    L.add(graph("Loop timings", series(E, [
        ("ORDERING_PROTOCOL_POLL_TIME", "op poll"),
        ("ORDERING_PROTOCOL_PROCESS_TIME", "op process"),
        ("REPLICA_INTERNAL_PROCESS_TIME", "replica internal"),
        ("REPLICA_TAKE_FROM_NETWORK", "take from network"),
        ("REPLICA_PROTOCOL_RESP_PROCESS_TIME", "protocol response"),
        ("RUN_LATENCY_TIME", "run latency"),
        ("DECISION_LOG_PROCESS_TIME", "decision log process"),
        ("DECISION_LOG_WORK_DELIVER_TIME", "decision log deliver"),
    ]), "ns",
        "Poll time climbing while process time stays flat means the replica is idle on "
        "the network rather than compute-bound."))
    L.add(graph("Queue depths", series(E, [
        ("REPLICA_RQ_QUEUE_SIZE", "replica rq queue"),
        ("DECISION_LOG_WORK_QUEUE_SIZE", "decision log queue"),
        ("MESSAGES_IN_CHANNEL", "messages in channel"),
    ]), "short",
        "A queue growing without bound identifies the bottleneck as the stage *after* it. "
        "All three are Trace-level, so this panel is empty outside the Trace suites."))
    L.add(graph("Messages processed", series(E, [
        ("OP_MESSAGES_PROCESSED", "ordering protocol"),
        ("DL_MESSAGES_PROCESSED", "decision log"),
        ("REPLICA_ORDERED_RQS_PROCESSED", "ordered rqs"),
        ("UNORDERED_OPS_PUSHED", "unordered ops"),
    ]), "cps"))
    L.add(graph("Timeouts", series(E, [
        ("TIMEOUT_RECEIVED_COUNT", "received"),
        ("TIMEOUT_MESSAGES_PROCESSED", "processed"),
        ("TIMEOUT_PROCESS_TIME", "process time"),
        ("TIMEOUT_MESSAGE_PROCESSING", "message processing"),
    ]), "short",
        "Counts and durations share the axis; the shape is what matters — any sustained "
        "rise here is the run leaving the happy path."))

    L.row("Communication",
          "atlas-communication and atlas-comm-mio. Signing cost scales with batch size, so "
          "read this row next to the protocol's batch panel before blaming the network.")
    L.add(graph("Serialization, signing and verification", series(E, [
        ("COMM_SERIALIZE_AND_SIGN_TIME", "serialize+sign"),
        ("COMM_DESERIALIZE_AND_VERIFY_TIME", "deserialize+verify"),
        ("MESSAGE_DELIVER_TIME", "deliver"),
        ("MESSAGE_DISPATCH_TIME", "dispatch"),
        ("MESSAGE_WAKER_TIME", "waker"),
        ("REQUEST_SEND_TIME", "request send"),
        ("THREADPOOL_PASS_TIME", "threadpool pass"),
    ]), "ns"))
    L.add(graph("Client pool", series(E, [
        ("CLIENT_POOL_COLLECT_TIME", "collect"),
        ("CLIENT_POOL_BATCH_PASSING_TIME", "batch passing"),
        ("CLIENT_POOL_SLEEP_TIME", "sleep"),
        ("TIME_SPENT_CLIENT_POOL", "total in pool"),
    ]), "ns"))
    L.add(graph("Message sizes", series(E, [
        ("OUTGOING_MESSAGE_SIZE", "outgoing"),
        ("INCOMING_MESSAGE_SIZE", "incoming"),
    ]), "bytes",
        "CountMax: each point is the largest message in that window, not an average."), w=24, h=7)

    L.row("Logging, checkpointing and transfer",
          "Durable log and the three transfer protocols. Spikes here that line up with "
          "throughput dips are the usual explanation for a periodic sawtooth.")
    L.add(graph("Checkpoint and digest", series(E, [
        ("DEC_LOG_CHECKPOINT_TIME", "decision log checkpoint"),
        ("APP_STATE_DIGEST_TIME", "app state digest"),
    ]), "ns"))
    L.add(graph("Transfer protocols", series(E, [
        ("LT_STATE_CLONE_TIME", "log transfer state clone"),
        ("LT_PROOFS_CLONE_TIME", "log transfer proofs clone"),
        ("STATE_TRANSFER_PROCESS_TIME", "state transfer process"),
        ("VT_MSG_PROCESS_TIME", "view transfer msg"),
    ]), "ns",
        "Note LT_STATE_CLONE_TIME is registered by both atlas-log-transfer and "
        "febft-state-transfer, so a single series can carry both crates' samples."))

    L.row("Host resources",
          "Written by the Atlas-Metrics OS thread every 250 ms. These bypass the metric "
          "registry and the level filter, so they are present in every suite.")
    L.add(graph("CPU per node", [target(q("OS_CPU_USER"), "$tag_host", "A")], "percentunit",
                "Averaged over cores: OS_CPU_USER is written once per core (tag `cpu`) "
                "as a 0..1 fraction."), w=8)
    L.add(graph("Process RSS per node", [target(q("OS_RAM_USAGE"), "$tag_host", "A")], "bytes",
                "Resident memory of the Atlas process, not the whole host."), w=8)
    L.add(graph("Network per node", [
        target(q("OS_NETWORK_UP", 'mean("value") * 4'), "OS_NETWORK_UP $tag_host", "A"),
        target(q("OS_NETWORK_DOWN", 'mean("value") * 4'), "OS_NETWORK_DOWN $tag_host", "B"),
    ], "Bps",
        "Bytes between 250 ms samples, scaled x4 to a per-second rate.\n\n"
        "Caveat: os_mon.rs crosses the directions — OS_NETWORK_UP is summed from "
        "received() and OS_NETWORK_DOWN from transmitted(). Trust the series names."), w=8)
    return L


# ── ordering protocols ────────────────────────────────────────────────────────────
def pbft_rows(L, E):
    """febft PBFT. Three-phase commit, so the interesting quantities are the two
    transitions between phases — that is where a quorum round trip actually lives."""
    L.row("PBFT — proposer",
          "febft-pbft-consensus: how batches get built before a round can start.")
    L.add(graph("Proposer timings", series(E, [
        ("PROPOSER_LATENCY", "proposer latency"),
        ("PROPOSER_PROPOSE_TIME", "propose time"),
        ("REQUEST_PROCESSING", "request processing"),
        ("REQUEST_FILTER_TIME", "request filter"),
        ("FWD_REQUEST_HANDLING", "forwarded request handling"),
    ]), "ns"))
    L.add(graph("Batching", series(E, [
        ("BATCH_SIZE", "consensus batch size"),
        ("CLIENT_POOL_BATCH_SIZE", "client pool batch size"),
        ("BATCHES_MADE", "batches made/s"),
        ("REQUESTS_COLLECTED", "requests collected/s"),
        ("PROPOSER_REQUEST_TIME_ITERATIONS", "proposer iterations/s"),
    ]), "short",
        "Batch size collapsing toward 1 while throughput drops is the classic sign the "
        "proposer is starved rather than saturated."))

    L.row("PBFT — consensus phases",
          "The pre-prepare → prepare → commit ladder. The two →-transitions each contain "
          "a quorum-wide round trip, so under a WAN profile they are where the added "
          "latency lands; the phase latencies themselves stay local.")
    L.add(graph("Phase ladder", series(E, [
        ("PROPOSE_LATENCY", "propose"),
        ("PRE_PREPARE_LATENCY", "pre-prepare"),
        ("PRE_PREPARE_TO_PREPARE_LATENCY", "pre-prepare→prepare"),
        ("PREPARE_LATENCY", "prepare"),
        ("PREPARE_TO_COMMIT_LATENCY", "prepare→commit"),
        ("COMMIT_LATENCY", "commit"),
    ]), "ns"))
    L.add(graph("Ordering throughput and pre-prepare analysis", series(E, [
        ("OPERATIONS_ORDERED", "operations ordered/s"),
        ("PRE_PREPARE_RQ_ANALYSIS", "pre-prepare rq analysis"),
        ("PRE_PREPARE_LOG_ANALYSIS", "pre-prepare log analysis"),
    ]), "short",
        "OPERATIONS_ORDERED is the protocol's own throughput — compare it against the "
        "executor's OPERATIONS_EXECUTED_PER_SECOND: a persistent gap means execution is "
        "falling behind consensus."))

    L.row("PBFT — view change and synchronisation",
          "Empty on a healthy run. Anything here means the cluster stopped making "
          "progress under the current leader.")
    L.add(graph("Synchronisation phase", series(E, [
        ("SYNC_REQUESTS_COUNT", "sync requests/s"),
        ("SYNC_FORWARDED_COUNT", "forwarded/s"),
        ("SYNC_WATCH_REQUESTS", "watch requests"),
        ("SYNC_STOPPED_REQUESTS", "stopped requests"),
        ("SYNC_BATCH_RECEIVED", "batch received"),
        ("SYNC_FORWARDED_REQUESTS", "forwarded requests"),
    ]), "short"))
    L.add(graph("State installation", series(E, [
        ("CONSENSUS_INSTALL_STATE_TIME", "consensus install state"),
        ("MSG_LOG_INSTALL_TIME", "message log install"),
    ]), "ns", "Cost of catching a replica up after it fell behind or restarted."))
    return L


def hotstuff_basic_rows(L, E):
    """Four-phase HotStuff: linear view change, one leader collecting threshold
    signatures per phase, so crypto cost sits on the critical path in a way it does
    not for PBFT's all-to-all rounds."""
    L.row("HotStuff — phase ladder",
          "hot-iron-oxide src/hot_iron/: prepare → pre-commit → commit → decide. Unlike "
          "PBFT's all-to-all rounds, each phase is a leader collecting votes into a "
          "quorum certificate, so a slow phase points at one node, not the mesh.")
    L.add(graph("Phase latencies", series(E, [
        ("PREPARE_LATENCY", "prepare"),
        ("PRE_COMMIT_LATENCY", "pre-commit"),
        ("COMMIT_LATENCY", "commit"),
        ("DECIDED_LATENCY", "decided"),
        ("FINALIZED_LATENCY", "finalized"),
    ]), "ns",
        "PREPARE_LATENCY and COMMIT_LATENCY are also febft metric names with entirely "
        "different meanings. If both protocols have written to this database, filter by "
        "Run before comparing."))
    L.add(graph("End-to-end and signing", series(E, [
        ("END_TO_END_LATENCY", "end-to-end"),
        ("SIGNATURE_PROPOSAL_LATENCY", "signature: proposal"),
        ("SIGNATURE_VOTE_LATENCY", "signature: vote"),
    ]), "ns",
        "Threshold-signature cost is on the critical path here. If the signature series "
        "track the phase latencies, the bottleneck is crypto, not consensus."))
    return L


def hotstuff_chained_rows(L, E):
    """Chained HotStuff: one pipelined generic phase, so there is no ladder — the unit
    of measurement is the vote/proposal round trip between consecutive heights."""
    L.row("Chained HotStuff — pipelined round",
          "hot-iron-oxide src/chained/: one generic phase per height, pipelined, so there "
          "is no four-phase ladder to plot. Note the roles differ — VOTE_SEND_LATENCY is "
          "recorded by every replica, while VOTE_RECEIVED and PROPOSAL_SEND are recorded "
          "only by the leader for that height, so expect them on one node at a time.")
    L.add(graph("Round trip", series(E, [
        ("VOTE_SEND_LATENCY", "proposal received → vote sent"),
        ("VOTE_RECEIVED_LATENCY", "first vote → last vote (leader)"),
        ("PROPOSAL_SEND_LATENCY", "last vote → proposal sent (leader)"),
        ("END_TO_END_LATENCY", "proposal received → finalized"),
    ]), "ns",
        "END_TO_END_LATENCY spans several pipelined heights, so it is legitimately much "
        "larger than the sum of the other three."))
    L.add(graph("Signing", series(E, [
        ("SIGNATURE_PROPOSAL_LATENCY", "signature: proposal"),
        ("SIGNATURE_VOTE_LATENCY", "signature: vote"),
    ]), "ns"))
    return L


# ── executors ─────────────────────────────────────────────────────────────────────
def standard_executor_rows(L, E, title="Execution (standard executor)"):
    L.row(title, "atlas-smr-execution: the baseline, which waits for the commit before "
                 "touching application state.")
    L.add(graph("Execution timings", series(E, [
        ("EXECUTION_LATENCY", "queue → execute"),
        ("EXECUTION_TIME_TAKEN", "application call"),
        ("UNORDERED_EXECUTION_TIME_TAKEN", "unordered application call"),
        ("REPLIES_PASSING_TIME", "replies passing"),
        ("REPLY_SENT_TIME", "reply sent"),
    ]), "ns",
        "EXECUTION_LATENCY is queue-to-execute; EXECUTION_TIME_TAKEN is the application "
        "itself. On a null-application suite the second should be near zero."))
    L.add(graph("Executed throughput", series(E, [
        ("OPERATIONS_EXECUTED_PER_SECOND", "ordered ops/s"),
        ("UNORDERED_OPERATIONS_EXECUTED_PER_SECOND", "unordered ops/s"),
    ]), "ops"))
    return L


def preemptive_executor_rows(L, E):
    L.row("Preemptive execution — the payoff",
          "atlas-smr-preemptive-execution: work starts on the proposal rather than the "
          "commit. Compare against CONSENSUS_WAIT_TIME — that dead window is exactly what "
          "speculation is trying to spend.")
    L.add(graph("Speculative execution time by variant", series(E, [
        ("CACHE_PREEMPTIVE_EXECUTION_TIME", "cache"),
        ("DS_PREEMPTIVE_EXECUTION_TIME", "double-state"),
        ("SCALABLE_PREEMPTIVE_EXECUTION_TIME", "scalable"),
        ("CONFIRM_EXECUTION_TIME", "confirm"),
        ("CACHE_CONFIRM_APPLICATION_TIME", "cache confirm apply"),
        ("CACHE_UNORDERED_EXECUTION_TIME", "cache unordered"),
    ]), "ns", "Only the variant the binary was built with reports; the rest stay empty."))
    L.add(graph("Speculation-to-confirm latency", series(E, [
        ("CACHE_SPECULATION_TO_CONFIRM_LATENCY", "cache"),
        ("DS_SPECULATION_TO_CONFIRM_LATENCY", "double-state"),
        ("CONFIRMED_WORKER_LATENCY", "confirmed worker"),
        ("CACHE_ENQUEUE_TO_EXECUTE_LATENCY", "enqueue→execute"),
    ]), "ns"))

    L.row("Preemptive execution — the cost",
          "Speculation is only worth it while this row stays quiet.")
    L.add(graph("Wasted work", series(E, [
        ("CACHE_BACKTRACK_COUNT", "cache backtracks/s"),
        ("DS_BACKTRACK_COUNT", "double-state backtracks/s"),
        ("SPECULATION_FALLBACK_COUNT", "fallbacks/s"),
        ("SCALABLE_COLLISION_COUNT", "scalable collisions"),
        ("SCALABLE_COLLISION_RATE", "scalable collision rate"),
    ]), "short",
        "Rising with load means the proposed and committed orders are diverging more often."))
    L.add(graph("Queues, buffers and batches", series(E, [
        ("CACHE_PENDING_QUEUE_SIZE", "cache pending queue"),
        ("REORDER_BUFFER_SIZE", "reorder buffer"),
        ("REORDER_STAGED_COUNT", "reorder staged/s"),
        ("CACHE_DELTA_SIZE", "cache delta size"),
        ("CACHE_OPS_PER_BATCH", "cache ops/batch"),
        ("DS_OPS_PER_BATCH", "double-state ops/batch"),
        ("SCALABLE_OPS_PER_BATCH", "scalable ops/batch"),
    ]), "short",
        "Batches arrive out of order under preemptive execution, so a non-empty reorder "
        "buffer is expected; one that grows without bound is not."))
    L.add(graph("Cache rebuild", series(E, [("CACHE_REBUILD_TIME", "rebuild")]), "ns",
                "Paid after a backtrack — the other half of the backtrack's true cost."),
          w=24, h=7)
    return L


def crud_rows(L, E):
    L.row("CRUD application",
          "The only suite with an instrumented application. Compare CRUD_OP_EXEC_TIME "
          "against the executor's EXECUTION_TIME_TAKEN: the gap is Atlas' per-batch "
          "overhead around the application.")
    L.add(graph("Application execution", series(E, [
        ("CRUD_BATCH_EXEC_TIME", "batch"),
        ("CRUD_OP_EXEC_TIME", "single op"),
    ]), "ns",
        "Workload shape (KEY_DISTRIBUTION, READ/CREATE/UPDATE/DELETE_RATIO, "
        "EXPENSIVE_OP_SLEEP_MS) moves the single-op line directly."))
    L.add(graph("Application throughput and batch size", series(E, [
        ("CRUD_CLIENT_OPS_DONE", "client ops done/s"),
        ("CRUD_OPS_PER_BATCH", "ops per batch"),
    ]), "short"))

    if "CRUD_CLIENT_LATENCY" in E:
        L.row("Per-request latency distribution",
              "CRUD_CLIENT_LATENCY is a CorrelationDurationTracker: one point per request "
              "rather than one average per second, which is what makes real percentiles "
              "possible here and nowhere else in the bench.")
        L.add(graph("Latency percentiles", [
            target(q("CRUD_CLIENT_LATENCY", 'mean("value")'), "mean $tag_host", "A"),
            target(q("CRUD_CLIENT_LATENCY", 'percentile("value", 50)'), "p50 $tag_host", "B"),
            target(q("CRUD_CLIENT_LATENCY", 'percentile("value", 95)'), "p95 $tag_host", "C"),
            target(q("CRUD_CLIENT_LATENCY", 'percentile("value", 99)'), "p99 $tag_host", "D"),
            target(q("CRUD_CLIENT_LATENCY", 'max("value")'), "max $tag_host", "E"),
        ], "ns",
            "A mean that stays flat while p99 climbs is the signature of a tail problem — "
            "queueing or a straggling replica — that an averaged metric would hide."), w=24)

    tracking = [m for m in ("RQ_CLIENT_TRACKING", "RQ_BATCH_TRACKING") if m in E]
    if tracking:
        L.row("Request tracking",
              "atlas-core Correlation metrics, registered Disabled by default and raised to "
              "Debug by this suite alone (crud_perf_exec/src/replica.rs). Each point is one "
              "event in one request's life, tagged with the stage it reached.")
        for m in tracking:
            L.add(graph(f"{m} — events by stage", [target(
                f'SELECT count("correlation_id") FROM "{m}" WHERE {SEL} '
                f'GROUP BY time($__interval), "event" fill(none)', "$tag_event", "A")], "cps",
                "Counts events per stage per second. Stages should move together; one "
                "flattening while the others continue is where requests are piling up."))
        if "RQ_CLIENT_TRACK_GLOBAL" in E:
            L.add(graph("Tracked request lifetime (aggregate)", series(E, [
                ("RQ_CLIENT_TRACK_GLOBAL", "mean tracked lifetime")]), "ns",
                "CorrelationAggrDurationTracker: the mean over all requests still being "
                "tracked in that window."))
    return L


# ── assembly ──────────────────────────────────────────────────────────────────────
PROTOCOL_ROWS = {
    "pbft": pbft_rows,
    "hotstuff-basic": hotstuff_basic_rows,
    "hotstuff-chained": hotstuff_chained_rows,
}
PROTOCOL_LABEL = {
    "pbft": "febft PBFT",
    "hotstuff-basic": "HotStuff (four-phase)",
    "hotstuff-chained": "Chained HotStuff",
}

TEMPLATING = {"list": [
    {"name": "host", "label": "Node", "type": "query", "datasource": DS,
     "query": 'SHOW TAG VALUES WITH KEY = "host"',
     "refresh": 2, "multi": True, "includeAll": True, "allValue": ".*",
     "current": {"selected": True, "text": ["All"], "value": ["$__all"]},
     "options": [], "sort": 1, "hide": 0,
     "description": "Atlas tags every point with host=NodeId(n): replicas are 0..n-1, "
                    "clients start at 1000. Keep multi/includeAll on — the values contain "
                    "parentheses, and Grafana only regex-escapes multi-value variables."},
    {"name": "run", "label": "Run", "type": "query", "datasource": DS,
     "query": 'SHOW TAG VALUES WITH KEY = "extra"',
     "refresh": 2, "multi": True, "includeAll": True, "allValue": ".*",
     "current": {"selected": True, "text": ["All"], "value": ["$__all"]},
     "options": [], "sort": 1, "hide": 0,
     "description": "The `extra` tag: 'None' unless the run set INFLUX_EXTRA. Suites share "
                    "one database and several measurement names, so filter by Run before "
                    "comparing anything across protocols."},
]}


def dashboard(uid, title, description, panels, tags):
    # Panel ids: Grafana assigns these on save, but a provisioned dashboard is never
    # saved, and without them ?viewPanel=/?editPanel= and per-panel share links fail
    # with "An unexpected error happened".
    for i, panel in enumerate(panels, start=1):
        panel["id"] = i
    return {
        "uid": uid, "title": title, "description": description, "tags": tags,
        "editable": True, "schemaVersion": 39, "version": 1, "refresh": "5s",
        "time": {"from": "now-15m", "to": "now"},
        "timepicker": {"refresh_intervals": ["1s", "5s", "10s", "30s", "1m", "5m"]},
        "timezone": "browser", "graphTooltip": 1,
        "templating": TEMPLATING, "panels": panels, "annotations": {"list": []},
    }


def headline(L, E, spec, name):
    """Top-of-dashboard stats plus the suite's own README, in the dashboard."""
    throughput = ("OPERATIONS_EXECUTED_PER_SECOND" if "OPERATIONS_EXECUTED_PER_SECOND" in E
                  else "OPERATIONS_ORDERED" if "OPERATIONS_ORDERED" in E else None)
    if throughput:
        L.add(stat("Throughput", q(throughput, by_host=False),
                   unit_of(throughput, "Counter"), decimals=0,
                   desc=("Mean across replicas. Every replica handles every operation, so "
                         "the mean is cluster throughput — a sum would multiply it by n.\n\n"
                         + ("Measured at the executor." if throughput.startswith("OPERATIONS_EXECUTED")
                            else "This suite registers no executor metrics, so throughput is "
                                 "read off the ordering protocol instead."))), w=5, h=5)
    if "CLIENT_RQ_PER_SECOND" in E:
        L.add(stat("Client request rate",
                   'SELECT sum("rate") FROM (SELECT mean("value") AS "rate" FROM '
                   f'"CLIENT_RQ_PER_SECOND" WHERE {SEL} GROUP BY time($__interval), "host") '
                   'GROUP BY time($__interval) fill(none)', "reqps", decimals=0,
                   desc="Summed over client processes — each drives a disjoint slice of the "
                        "load, so here the sum is the real total."), w=5, h=5)
    if "CLIENT_RQ_LATENCY" in E:
        L.add(stat("Client latency", q("CLIENT_RQ_LATENCY", by_host=False), "ns",
                   desc="End-to-end at the client: send → f+1 matching replies."), w=5, h=5)
    if "CONSENSUS_WAIT_TIME" in E:
        L.add(stat("Consensus wait", q("CONSENSUS_WAIT_TIME", by_host=False), "ns",
                   desc="Pre-prepare → handed to the executor. Roughly a consensus round trip "
                        "for a standard executor, near zero for a preemptive one."), w=4, h=5)

    supp = spec["_suppressed"]
    note = [
        f"**{spec['make']}** — {PROTOCOL_LABEL[spec['protocol']]}, "
        f"replica metrics at `MetricLevel::{spec['replica_level']}`, "
        f"clients at `MetricLevel::{spec['client_level']}`.",
        "", spec["blurb"], "",
    ]
    if supp:
        note += [
            "---",
            f"**Registered but not emitted here** ({len(supp)}). This suite passes these "
            f"crates' `metrics()` to `initialize_metrics`, but `with_metric_level"
            f"(MetricLevel::{spec['replica_level']})` filters them out, so no panel below "
            "draws them — raise the level in the binary if you need one:",
            "",
            ", ".join(f"`{m}` ({lvl})" for m, lvl in sorted(supp.items())),
        ]
    # Size the note to its content: a full-width markdown line wraps at roughly 110
    # characters, and a panel that clips its own text is worse than one slightly too tall.
    body = "\n".join(note)
    lines = sum(max(1, len(ln) // 110 + 1) for ln in body.split("\n"))
    L.add(text_panel("About this suite", body), w=24, h=max(8, 3 + lines))
    return L


def build_suite(name, spec):
    E = dict(emitted(spec["replica_crates"], spec["replica_level"]))
    E.update(emitted(spec["client_crates"], spec["client_level"]))
    for m, lvl in spec.get("replica_level_overrides", {}).items():
        if LEVEL_ORD[lvl] >= LEVEL_ORD[spec["replica_level"]]:
            for crate in spec["replica_crates"]:
                if m in REG[crate]:
                    E[m] = (REG[crate][m][0], lvl)
    E.update(OS_METRICS)

    supp = suppressed(spec["replica_crates"], spec["replica_level"])
    supp.update(suppressed(spec["client_crates"], spec["client_level"]))
    for m in list(supp):
        if m in E:
            del supp[m]
    spec["_suppressed"] = supp

    L = Layout()
    headline(L, E, spec, name)
    PROTOCOL_ROWS[spec["protocol"]](L, E)
    if spec["executor"] in ("standard", "both"):
        standard_executor_rows(L, E)
    if spec["executor"] in ("preemptive", "both"):
        preemptive_executor_rows(L, E)
    if "crud-app" in spec["replica_crates"]:
        crud_rows(L, E)
    baseline_rows(L, E)

    desc = (f"{spec['make']} — {PROTOCOL_LABEL[spec['protocol']]}. "
            f"Panels are generated against the metrics this suite actually emits; see the "
            f"'About this suite' note for what its metric level leaves out.")
    return dashboard(spec["uid"], spec["title"], desc, L.panels,
                     ["atlas", "bench", "suite", spec["protocol"]]), E, supp


def build_overview():
    """Cross-suite entry point: only measurements every suite can produce."""
    E = {m: ("Counter", "Info") for m in
         ("OPERATIONS_EXECUTED_PER_SECOND", "OPERATIONS_ORDERED", "CLIENT_RQ_PER_SECOND",
          "CLIENT_RQ_RECV_PER_SECOND", "CLIENT_RQ_TIMEOUT")}
    E.update({m: ("Duration", "Info") for m in
              ("CLIENT_RQ_LATENCY", "CLIENT_UNORDERED_RQ_LATENCY", "CONSENSUS_WAIT_TIME",
               "EXECUTION_LATENCY", "EXECUTION_TIME_TAKEN", "REPLY_SENT_TIME",
               "END_TO_END_LATENCY")})
    E.update({m: ("Count", "Info") for m in
              ("BATCH_SIZE", "RQ_PRE_PROCESSING_BATCH_SIZE", "CLIENT_POOL_BATCH_SIZE")})
    E.update(OS_METRICS)

    L = Layout()
    L.add(stat("Ordered throughput", q("OPERATIONS_EXECUTED_PER_SECOND", by_host=False),
               "ops", decimals=0,
               desc="Mean across replicas. Empty for suites with no executor metrics "
                    "(microbenchmarks legacy) — use the per-suite dashboard there."), w=6, h=5)
    L.add(stat("Client request rate",
               'SELECT sum("rate") FROM (SELECT mean("value") AS "rate" FROM '
               f'"CLIENT_RQ_PER_SECOND" WHERE {SEL} GROUP BY time($__interval), "host") '
               'GROUP BY time($__interval) fill(none)', "reqps", decimals=0), w=6, h=5)
    L.add(stat("Client latency", q("CLIENT_RQ_LATENCY", by_host=False), "ns"), w=6, h=5)
    L.add(stat("Consensus wait", q("CONSENSUS_WAIT_TIME", by_host=False), "ns"), w=6, h=5)
    L.add(text_panel("Which dashboard do I want?", "\n".join([
        "This is the cross-suite view: it draws only what **every** suite can produce, so a "
        "blank panel here usually means the running suite does not register that metric "
        "rather than that anything is wrong. For the full picture pick the suite's own "
        "dashboard in this folder:", "",
        "| Dashboard | `make` target | Ordering protocol |",
        "|---|---|---|",
    ] + [f"| {s['title']} | `{s['make']}` | {PROTOCOL_LABEL[s['protocol']]} |"
         for s in SUITES.values()] + [
        "",
        "All suites write to one database and several measurement names are shared between "
        "protocols (`PREPARE_LATENCY` and `COMMIT_LATENCY` mean different things in febft and "
        "HotStuff), so tag runs with `INFLUX_EXTRA` and use the **Run** filter before "
        "comparing anything across suites.",
    ])), w=24, h=13)

    L.row("Throughput and latency")
    L.add(graph("Throughput per replica", series(E, [
        ("OPERATIONS_EXECUTED_PER_SECOND", "executed"),
        ("OPERATIONS_ORDERED", "ordered"),
    ]), "ops", "Ordered but not executed means execution is falling behind consensus."))
    L.add(graph("Client rate and timeouts", series(E, [
        ("CLIENT_RQ_PER_SECOND", "sent"),
        ("CLIENT_RQ_RECV_PER_SECOND", "received"),
        ("CLIENT_RQ_TIMEOUT", "timeouts"),
    ]), "reqps"))
    L.add(graph("Client latency", series(E, [
        ("CLIENT_RQ_LATENCY", "ordered"),
        ("CLIENT_UNORDERED_RQ_LATENCY", "unordered"),
    ]), "ns"))
    L.add(graph("Replica-side latency breakdown", series(E, [
        ("CONSENSUS_WAIT_TIME", "consensus wait"),
        ("EXECUTION_LATENCY", "execution latency"),
        ("EXECUTION_TIME_TAKEN", "execution time"),
        ("REPLY_SENT_TIME", "reply send"),
        ("END_TO_END_LATENCY", "end-to-end (HotStuff)"),
    ]), "ns"))
    L.add(graph("Batch size", series(E, [
        ("BATCH_SIZE", "consensus batch"),
        ("RQ_PRE_PROCESSING_BATCH_SIZE", "pre-processing batch"),
        ("CLIENT_POOL_BATCH_SIZE", "client pool batch"),
    ]), "short"), w=24, h=8)

    L.row("Host resources")
    L.add(graph("CPU per node", [target(q("OS_CPU_USER"), "$tag_host", "A")], "percentunit",
                "Averaged over cores; OS_CPU_USER is a 0..1 fraction per core."), w=8)
    L.add(graph("Process RSS per node", [target(q("OS_RAM_USAGE"), "$tag_host", "A")], "bytes"), w=8)
    L.add(graph("Network per node", [
        target(q("OS_NETWORK_UP", 'mean("value") * 4'), "OS_NETWORK_UP $tag_host", "A"),
        target(q("OS_NETWORK_DOWN", 'mean("value") * 4'), "OS_NETWORK_DOWN $tag_host", "B"),
    ], "Bps", "os_mon.rs crosses the directions — trust the series names, not up/down."), w=8)

    return dashboard("atlas-overview", "Atlas — Cluster Overview",
                     "Cross-suite entry point. Start here, then open the dashboard for the "
                     "suite you are actually running.", L.panels,
                     ["atlas", "bench", "overview"])


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    written = []

    d = build_overview()
    path = os.path.join(OUT_DIR, "atlas-cluster-overview.json")
    json.dump(d, open(path, "w"), indent=2)
    written.append((os.path.basename(path), d["title"], len(d["panels"]), "", ""))

    for name, spec in SUITES.items():
        d, E, supp = build_suite(name, spec)
        path = os.path.join(OUT_DIR, f"suite-{name}.json")
        json.dump(d, open(path, "w"), indent=2)
        drawn = len([p for p in d["panels"] if p["type"] not in ("row", "text")])
        written.append((os.path.basename(path), d["title"], drawn, len(E), len(supp)))

    w = max(len(f) for f, *_ in written)
    print(f"{'file'.ljust(w)}  panels  emitted  suppressed")
    for f, title, panels, e, s in written:
        print(f"{f.ljust(w)}  {panels:>6}  {str(e):>7}  {str(s):>10}")
    print(f"\n{len(written)} dashboards → {OUT_DIR}")


if __name__ == "__main__":
    main()
