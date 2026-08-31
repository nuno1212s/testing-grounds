#!/usr/bin/env python3
"""Compare executor variants from a benchmark run.

Each variant writes to the same InfluxDB, tagged with `extra` (set automatically from the
compiled-in executor, see common.rs::EXECUTOR_VARIANT). This pulls the metrics that matter
for the preemptive-vs-baseline comparison and prints one table.

Usage:
    ./compare_variants.py --url http://localhost:8086 --db atlas
    ./compare_variants.py --since 30m --variants baseline,crud_scalable

Note on statistics: atlas-metrics stores rolling mean/stddev (Welford), not histograms, so
there are no percentiles available. Latency claims from this table are mean +/- stddev only.
"""

import argparse
import json
import sys
import urllib.parse
import urllib.request
from collections import OrderedDict

# (measurement, label, unit). Duration metrics are stored in nanoseconds.
LATENCY_METRICS = [
    ("CONSENSUS_WAIT_TIME", "Consensus wait", "ns"),
    ("CLIENT_RQ_LATENCY", "E2E latency (client)", "ns"),
    ("RQ_CLIENT_TRACK_GLOBAL", "E2E latency (server)", "ns"),
]

# Execution time is reported under a different metric name per variant, because the baseline
# and preemptive crates deliberately reuse the same numeric IDs.
EXEC_METRICS = [
    ("EXECUTION_TIME_TAKEN", "Execution (baseline)", "ns"),
    ("CACHE_PREEMPTIVE_EXECUTION_TIME", "Execution (cache spec)", "ns"),
    ("DS_PREEMPTIVE_EXECUTION_TIME", "Execution (dual-state spec)", "ns"),
    ("SCALABLE_PREEMPTIVE_EXECUTION_TIME", "Execution (scalable spec)", "ns"),
    ("CONFIRM_EXECUTION_TIME", "Confirm re-exec (dual-state)", "ns"),
    ("CACHE_CONFIRM_APPLICATION_TIME", "Confirm delta apply (cache)", "ns"),
]

# The cost side of speculation: if these are high, a latency win is not free.
OVERHEAD_METRICS = [
    ("DS_BACKTRACK_COUNT", "Backtracks (dual-state)", "count"),
    ("CACHE_BACKTRACK_COUNT", "Backtracks (cache)", "count"),
    ("SCALABLE_COLLISION_RATE", "Collision rate (permille)", "permille"),
    ("CACHE_PENDING_QUEUE_SIZE", "Pending queue depth", "count"),
    ("OS_CPU_USER", "CPU user", "%"),
    ("OS_RAM_USAGE", "RAM", "bytes"),
]


_reported_errors = set()


def query(url, db, q, user=None, password=None):
    params = {"db": db, "q": q}
    if user:
        params["u"] = user
    if password:
        params["p"] = password
    endpoint = f"{url.rstrip('/')}/query?" + urllib.parse.urlencode(params)
    try:
        with urllib.request.urlopen(endpoint, timeout=30) as resp:
            return json.load(resp)
    except Exception as exc:  # noqa: BLE001 - surfaced to the user verbatim
        # One query runs per metric, so a down/unreachable server would otherwise print the
        # same error a dozen times. Report each distinct failure once.
        message = f"query failed: {exc}"
        if message not in _reported_errors:
            _reported_errors.add(message)
            print(message, file=sys.stderr)
        return None


def fetch(url, db, measurement, since, user, password):
    """Return {variant: (mean, stddev)} for one measurement."""
    q = (
        f'SELECT mean("value"), stddev("value") FROM "{measurement}" '
        f"WHERE time > now() - {since} GROUP BY \"extra\""
    )
    data = query(url, db, q, user, password)
    if not data:
        return {}

    out = {}
    for result in data.get("results", []):
        for series in result.get("series", []):
            variant = series.get("tags", {}).get("extra") or "(untagged)"
            cols = series["columns"]
            row = series["values"][0]
            point = dict(zip(cols, row))
            out[variant] = (point.get("mean"), point.get("stddev"))
    return out


def fmt(value, unit):
    if value is None:
        return "-"
    if unit == "ns":
        # Show in the most readable unit; these span microseconds to seconds.
        if value >= 1e9:
            return f"{value / 1e9:.3f} s"
        if value >= 1e6:
            return f"{value / 1e6:.3f} ms"
        if value >= 1e3:
            return f"{value / 1e3:.3f} us"
        return f"{value:.0f} ns"
    if unit == "bytes":
        return f"{value / (1024 ** 2):.1f} MiB"
    if unit == "permille":
        return f"{value / 10:.2f} %"
    return f"{value:,.2f}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://localhost:8086")
    parser.add_argument("--db", default="atlas")
    parser.add_argument("--user")
    parser.add_argument("--password")
    parser.add_argument("--since", default="1h", help="InfluxQL duration, e.g. 30m, 2h")
    parser.add_argument(
        "--variants",
        default="baseline,dual_state,crud_single,crud_scalable",
        help="comma-separated, in column order",
    )
    args = parser.parse_args()

    variants = [v.strip() for v in args.variants.split(",") if v.strip()]
    sections = [
        ("Latency", LATENCY_METRICS),
        ("Execution", EXEC_METRICS),
        ("Speculation cost / resources", OVERHEAD_METRICS),
    ]

    rows = OrderedDict()
    for _, metrics in sections:
        for measurement, _, _ in metrics:
            rows[measurement] = fetch(
                args.url, args.db, measurement, args.since, args.user, args.password
            )

    seen = {v for data in rows.values() for v in data}
    if not seen:
        print(
            f"No data found in the last {args.since}. Check --url/--db, and that the "
            f"replicas ran with metrics enabled.",
            file=sys.stderr,
        )
        return 1

    unexpected = seen - set(variants)
    if unexpected:
        print(f"note: untracked variant tags present: {', '.join(sorted(unexpected))}\n")

    label_w = 30
    col_w = 22
    for title, metrics in sections:
        print(f"\n=== {title} ===")
        print("metric".ljust(label_w) + "".join(v.ljust(col_w) for v in variants))
        print("-" * (label_w + col_w * len(variants)))
        for measurement, label, unit in metrics:
            data = rows.get(measurement, {})
            if not data:
                continue
            line = label.ljust(label_w)
            for variant in variants:
                mean, stddev = data.get(variant, (None, None))
                cell = fmt(mean, unit)
                if mean is not None and stddev:
                    cell += f" ±{fmt(stddev, unit)}"
                line += cell.ljust(col_w)
            print(line)

    print(
        "\nmean ±stddev only -- atlas-metrics uses Welford's online algorithm and stores no "
        "histogram, so percentiles are not available."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
