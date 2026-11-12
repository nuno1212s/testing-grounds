#!/usr/bin/env python3
"""Turn a sweep's runs.jsonl into tables.

    analyze_suite.py DIR [--x FACTOR] [--include-invalid]

Writes report.md, summary.csv (cell x variant, mean and 95% CI) and runs_flat.csv (one row per
run). Runs failing a validity gate (`validity()`; chiefly, speculation did not engage) are
listed and excluded. Savings are paired within a block; p99 needs >= 1000 samples."""

import argparse
import csv
import datetime as dt
import json
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import plan as planlib  # noqa: E402
from lib.stats import fmt_ci, mean_ci, paired_diff  # noqa: E402

SPECULATIVE = set(planlib.SPECULATIVE_VARIANTS)
MIN_SAMPLES_FOR_P99 = 1000
MAX_WINDOW_SPREAD = 0.1  # of the measurement window
HIT_RATE_FLOOR = 900  # permille: below this, speculation did not carry the run
MAX_EXECUTOR_BUSY = 0.5  # above this, queueing behind other requests can hide what speculation saves
SCALABLE_EXEC_THREADS = 4  # PARALLEL_EXEC_THREAD_POOL_SIZE in atlas-smr-preemptive-execution

LATENCY_METRICS = ["mean_ms", "p50_ms", "p99_ms"]
ALL_METRICS = ["p50_ms", "p90_ms", "p99_ms", "p999_ms", "mean_ms", "max_ms", "throughput"]


# ── per-run extraction and validation ─────────────────────────────────────────────────

def load_runs(path):
    runs = []
    with open(path) as f:
        for line in f:
            if line.strip():
                runs.append(json.loads(line))
    # A run retried after a failure appears twice; the last record of a run_id wins.
    latest = {}
    for r in runs:
        latest[r["run_id"]] = r
    return list(latest.values())


def _metric(run, name):
    m = (run.get("replica_metrics") or {}).get(name)
    return None if m is None else m["value"]


def extract(run):
    """Flat numbers for one run, None where the run does not have them."""
    agg = run.get("agg") or {}
    ordered = agg.get("ordered") or {}
    flat = {
        "throughput": agg.get("throughput"),
        "n_samples": ordered.get("count", 0),
    }
    for k in ("p50_ms", "p90_ms", "p99_ms", "p999_ms", "mean_ms", "max_ms"):
        flat[k] = ordered.get(k)

    flat["hit_rate"] = _metric(run, "SPECULATION_HIT_RATE")
    ds, cache = _metric(run, "DS_BACKTRACK_COUNT"), _metric(run, "CACHE_BACKTRACK_COUNT")
    flat["backtracks"] = None if (ds is None and cache is None) else (ds or 0) + (cache or 0)
    flat["batch_size"] = _metric(run, "BATCH_SIZE")
    cw = _metric(run, "CONSENSUS_WAIT_TIME")
    flat["consensus_wait_ms"] = None if cw is None else cw / 1e6
    cr = _metric(run, "CONFIRM_TO_REPLY_TIME")
    flat["confirm_to_reply_ms"] = None if cr is None else cr / 1e6
    flat["replica_cpu"] = _metric(run, "OS_CPU_USER")

    rtts = [v for v in (run.get("rtt_measured_ms") or {}).values() if v is not None]
    flat["rtt_measured_ms"] = sum(rtts) / len(rtts) if rtts else None
    return flat


def window_spread_s(run):
    """Seconds between the earliest and latest client measurement-window start."""
    starts = []
    for c in run.get("clients") or []:
        if c.get("_ts") and c.get("elapsed_s") is not None:
            ts = dt.datetime.fromisoformat(c["_ts"].replace("Z", "+00:00"))
            starts.append(ts - dt.timedelta(seconds=float(c["elapsed_s"])))
    return (max(starts) - min(starts)).total_seconds() if len(starts) > 1 else 0.0


def validity(run, plan_fixed=None):
    """(problems, warnings). Any problem excludes the run from the statistics."""
    problems, warnings = [], []
    variant = run["variant"]

    if run.get("status") != "ok":
        return [f"run did not complete ({run.get('status')}: {run.get('error', '')[:120]})"], []

    flat = extract(run)
    agg = run.get("agg") or {}

    if plan_fixed:
        expected = int(plan_fixed["N_CLIENTS"]) * int(plan_fixed.get("N_CLIENT_MACHINES", 1))
        if agg.get("clients") != expected:
            problems.append(f"{agg.get('clients')} of {expected} clients reported")

    if not agg.get("ops") or not flat["n_samples"]:
        problems.append("no operations completed in the measurement window")

    # Throughput sums per-client rates, which is only valid if the windows overlap.
    spread, measure = window_spread_s(run), (run.get("clients") or [{}])[0].get("measure_s") or 0
    if measure and spread > MAX_WINDOW_SPREAD * measure:
        problems.append(f"client windows start {spread:.1f}s apart (> {MAX_WINDOW_SPREAD:.0%} of {measure:g}s)")

    restarts = {k: v for k, v in (run.get("replica_restarts") or {}).items() if v}
    if restarts:
        problems.append(f"replica restarted (a crash was masked by restart: on-failure): {restarts}")

    tag = variant.replace("_", "-")
    images = run.get("images") or {}
    if images:
        wrong = {n: i for n, i in images.items() if tag not in i}
        if wrong:
            problems.append(f"image does not match variant {variant}: {wrong}")
    else:
        warnings.append("container images not recorded; variant identity unverified")

    nominal = (run.get("factors") or {}).get("rtt_ms")
    measured = flat["rtt_measured_ms"]
    if nominal is not None:
        if measured is None:
            warnings.append("replica RTT not measured")
        elif abs(measured - nominal) > 0.6 + 0.15 * nominal:
            problems.append(f"measured replica RTT {measured:.2f} ms vs profile {nominal} ms")

    hit, backtracks = flat["hit_rate"], flat["backtracks"]
    if variant in SPECULATIVE:
        if hit is None:
            warnings.append("SPECULATION_HIT_RATE missing: could not confirm speculation engaged")
        elif hit < HIT_RATE_FLOOR:
            problems.append(f"speculation did not carry the run (hit rate {hit:.0f}/1000)")
        if backtracks:
            problems.append(f"{backtracks:.0f} backtracks in a fault-free steady-state run")
        elif backtracks is None:
            warnings.append("backtrack counters missing")
    elif hit:
        problems.append(f"baseline run reports speculation (hit rate {hit:.0f}): variant mix-up")

    factors = run.get("factors") or {}
    cost_us = factors.get("op_cost_us")
    fixed = plan_fixed or {}
    in_flight = (int(factors.get("N_CLIENTS", fixed.get("N_CLIENTS", 1)))
                 * int(factors.get("CONCURRENT_RQS", fixed.get("CONCURRENT_RQS", 1))))
    # A single outstanding request never waits behind another, however busy the executor is.
    if cost_us and flat["throughput"] and in_flight > 1:
        threads = SCALABLE_EXEC_THREADS if variant == "crud_scalable" else 1
        busy = flat["throughput"] * cost_us / 1e6 / threads
        if busy > MAX_EXECUTOR_BUSY:
            warnings.append(f"executor ~{busy:.0%} busy (> {MAX_EXECUTOR_BUSY:.0%}): queueing "
                            "behind other requests can hide the speculation saving")

    drops = sum((run.get("startup_drops") or {}).values())
    if drops:
        warnings.append(f"start-up race: {drops} client messages dropped as unauthenticated "
                        "(masked by the quorum)")

    if "replica_metrics_error" in run:
        warnings.append(f"InfluxDB query failed: {run['replica_metrics_error'][:80]}")

    return problems, warnings


# ── grouping and statistics ───────────────────────────────────────────────────────────

def cell_key(run):
    return tuple(sorted((run.get("factors") or {}).items()))


def group(runs, include_invalid, plan_fixed):
    """cells[cell_key][variant][rep] = flat metrics, for valid runs; also the verdict list."""
    cells, verdicts = {}, []
    for run in runs:
        problems, warnings = validity(run, plan_fixed)
        flat = extract(run)
        verdicts.append({"run": run, "flat": flat, "problems": problems, "warnings": warnings})
        if problems and not include_invalid:
            continue
        cells.setdefault(cell_key(run), {}).setdefault(run["variant"], {})[run["rep"]] = flat
    return cells, verdicts


def series(cell, variant, metric):
    return [m.get(metric) for _, m in sorted(cell.get(variant, {}).items())]


def paired_saving(cell, variant, metric, baseline="baseline"):
    """Per-rep (baseline - variant) for the reps both have, as (mean, ci, n)."""
    reps = sorted(set(cell.get(baseline, {})) & set(cell.get(variant, {})))
    a = [cell[baseline][r].get(metric) for r in reps]
    b = [cell[variant][r].get(metric) for r in reps]
    return paired_diff(a, b)


def predicted_saving_ms(variant, factors, flat_variant):
    """First-order model: saving per batch = min(batch execution time, replica RTT), since
    speculation overlaps execution only with the prepare+commit rounds. Ignores queueing."""
    p_ms = factors.get("op_cost_us")
    rtt = flat_variant.get("rtt_measured_ms") or factors.get("rtt_ms")
    b = flat_variant.get("batch_size")
    if p_ms is None or rtt is None or not b:
        return None
    p_ms /= 1000.0
    width = SCALABLE_EXEC_THREADS if variant == "crud_scalable" else 1
    exec_ms = math.ceil(b / width) * p_ms
    return min(exec_ms, rtt)


# ── rendering ─────────────────────────────────────────────────────────────────────────

def _factor_values(cells, name):
    vals = {dict(k).get(name) for k in cells}
    return sorted(v for v in vals if v is not None)


def md_table(header, rows):
    out = ["| " + " | ".join(header) + " |", "|" + "|".join("---" for _ in header) + "|"]
    out += ["| " + " | ".join(str(c) for c in r) + " |" for r in rows]
    return "\n".join(out)


def render_metric_tables(cells, variants, x, metric, unit, digits):
    """One table per slice (combination of all factors other than x)."""
    names = sorted({n for k in cells for n, _ in k})
    others = [n for n in names if n != x]
    slices = sorted({tuple((n, dict(k).get(n)) for n in others) for k in cells}, key=str)

    out = []
    for sl in slices:
        label = ", ".join(f"{n}={v}" for n, v in sl) or "all cells"
        cmp_label = "throughput ÷ baseline" if metric == "throughput" else "saving vs baseline"
        header = [x] + [v for v in variants] + [f"{cmp_label}: {v}" for v in variants if v != "baseline"]
        rows = []
        for xv in _factor_values(cells, x):
            key = tuple(sorted(((x, xv),) + sl))
            cell = cells.get(key)
            if not cell:
                continue
            row = [xv]
            starred = False
            for v in variants:
                vals = series(cell, v, metric)
                mean, half, n = mean_ci(vals)
                if metric == "p99_ms":
                    low = any((m.get("n_samples") or 0) < MIN_SAMPLES_FOR_P99
                              for m in cell.get(v, {}).values())
                    starred = starred or low
                    cellstr = fmt_ci(mean, half, digits) + (" †" if low and mean is not None else "")
                else:
                    cellstr = fmt_ci(mean, half, digits)
                row.append(cellstr + (f" (n={n})" if n else ""))
            for v in variants:
                if v == "baseline":
                    continue
                if metric == "throughput":
                    # for throughput, "saving" is the ratio of means: higher is better
                    b = mean_ci(series(cell, "baseline", metric))[0]
                    m = mean_ci(series(cell, v, metric))[0]
                    row.append("n/a" if not b or m is None else f"{m / b:.2f}x")
                else:
                    d, dh, dn = paired_saving(cell, v, metric)
                    b = mean_ci(series(cell, "baseline", metric))[0]
                    if d is None:
                        row.append("n/a")
                    else:
                        pct = f" ({100 * d / b:+.0f}%)" if b else ""
                        row.append(fmt_ci(d, dh, digits) + pct)
            rows.append(row)
        if rows:
            out.append(f"**{label}**  ({unit})\n\n{md_table(header, rows)}")
    return "\n\n".join(out)


def render_model_table(cells, variants):
    rows = []
    for key in sorted(cells, key=str):
        factors = dict(key)
        if "op_cost_us" not in factors:
            continue
        cell = cells[key]
        for v in variants:
            if v == "baseline" or v not in cell:
                continue
            d, dh, dn = paired_saving(cell, v, "p50_ms")
            dm, dmh, _ = paired_saving(cell, v, "mean_ms")
            flat = next(iter(cell[v].values()))
            pred = predicted_saving_ms(v, factors, flat)
            if d is None or pred is None:
                continue
            rows.append([factors.get("rtt_ms", "-"), factors["op_cost_us"], v,
                         f"{flat.get('batch_size') or 0:.1f}", f"{pred:.2f}",
                         fmt_ci(dm, dmh, 2), "n/a" if pred == 0 or dm is None else f"{dm / pred:.2f}",
                         fmt_ci(d, dh, 2)])
    if not rows:
        return ""
    return md_table(["rtt_ms", "op_cost_us", "variant", "mean batch", "model saving (ms)",
                     "measured mean saving (ms)", "mean / model", "measured p50 saving (ms)"], rows)


def render(plan_meta, cells, verdicts, variants, x, include_invalid):
    ok = sum(1 for v in verdicts if not v["problems"])
    bad = [v for v in verdicts if v["problems"]]
    warned = [v for v in verdicts if v["warnings"] and not v["problems"]]

    L = [f"# {plan_meta.get('plan', {}).get('name', 'sweep')}", ""]
    desc = (plan_meta.get("plan", {}) or {}).get("description")
    if desc:
        L += [" ".join(str(desc).split()), ""]

    prov = plan_meta.get("provenance") or {}
    L += [f"- runs: {len(verdicts)} recorded, **{ok} valid**, {len(bad)} excluded, "
          f"{len(warned)} valid with warnings",
          f"- host: {prov.get('cpu_model', '?')}, {prov.get('host_cpus', '?')} cores, "
          f"kernel {prov.get('kernel', '?')}, docker {prov.get('docker', '?')}",
          "- revisions: " + ", ".join(
              f"{k}@{(v or {}).get('rev', '?')}" + (f" (+{v['uncommitted_files']} uncommitted)"
                                                   if v and v.get('uncommitted_files') else "")
              for k, v in (prov.get("repos") or {}).items()),
          f"- statistics: mean ± Student-t 95% CI over valid repetitions; savings are paired "
          f"within a block. `†` = fewer than {MIN_SAMPLES_FOR_P99} samples in some run, so the "
          "p99 is not reliable.", ""]

    if bad:
        L += ["## Excluded runs", "",
              md_table(["run", "variant", "cell", "reason"],
                       [[v["run"]["run_id"], v["run"]["variant"], json.dumps(v["run"].get("factors")),
                         "; ".join(v["problems"])] for v in bad]), ""]
    if warned:
        L += ["## Warnings (runs kept)", "",
              md_table(["run", "warning"],
                       [[v["run"]["run_id"], "; ".join(v["warnings"])] for v in warned]), ""]

    L += ["## Latency of the ordered path, mean", "",
          "The mean is exact (the histogram keeps the sum); percentiles are quantized to a "
          "bucket, up to 1.6%, which is as large as a few-millisecond saving.", "",
          render_metric_tables(cells, variants, x, "mean_ms", "ms, lower is better", 2), "",
          "## Latency of the ordered path, p50", "",
          render_metric_tables(cells, variants, x, "p50_ms", "ms, lower is better", 2), "",
          "## Latency of the ordered path, p99", "",
          render_metric_tables(cells, variants, x, "p99_ms", "ms, lower is better", 2), "",
          "## Throughput", "",
          render_metric_tables(cells, variants, x, "throughput",
                               "ops/s; the last columns are ratios to baseline", 0), ""]

    model = render_model_table(cells, variants)
    if model:
        L += ["## Measured saving against the first-order model", "",
              "The model is min(batch execution time, replica RTT): speculation can overlap "
              "execution with at most the prepare+commit rounds (one replica round trip). "
              "See `predicted_saving_ms` for the assumptions.", "", model, ""]
    return "\n".join(L)


def write_csvs(out_dir, cells, verdicts, variants):
    factor_names = sorted({n for k in cells for n, _ in k})

    with open(os.path.join(out_dir, "summary.csv"), "w", newline="") as f:
        w = csv.writer(f)
        header = factor_names + ["variant", "n_valid"]
        for m in ALL_METRICS:
            header += [m, f"{m}_ci95"]
        header += ["saving_p50_ms", "saving_p50_ci95", "saving_p99_ms", "saving_p99_ci95"]
        w.writerow(header)
        for key in sorted(cells, key=str):
            cell = cells[key]
            fac = dict(key)
            for v in variants:
                if v not in cell:
                    continue
                row = [fac.get(n) for n in factor_names] + [v, len(cell[v])]
                for m in ALL_METRICS:
                    mean, half, _ = mean_ci(series(cell, v, m))
                    row += ["" if mean is None else f"{mean:.6g}", "" if half is None else f"{half:.6g}"]
                for m in ("p50_ms", "p99_ms"):
                    if v == "baseline":
                        row += ["", ""]
                    else:
                        d, dh, _ = paired_saving(cell, v, m)
                        row += ["" if d is None else f"{d:.6g}", "" if dh is None else f"{dh:.6g}"]
                w.writerow(row)

    with open(os.path.join(out_dir, "runs_flat.csv"), "w", newline="") as f:
        w = csv.writer(f)
        keys = ["throughput", "n_samples", "p50_ms", "p90_ms", "p99_ms", "p999_ms", "mean_ms",
                "max_ms", "hit_rate", "backtracks", "batch_size", "consensus_wait_ms",
                "confirm_to_reply_ms", "replica_cpu", "rtt_measured_ms"]
        w.writerow(["run_id", "rep", "variant"] + factor_names + keys + ["valid", "problems", "warnings"])
        for v in sorted(verdicts, key=lambda v: v["run"]["idx"]):
            r, flat = v["run"], v["flat"]
            fac = r.get("factors") or {}
            w.writerow([r["run_id"], r["rep"], r["variant"]] + [fac.get(n) for n in factor_names]
                       + [flat.get(k) for k in keys]
                       + [not v["problems"], "; ".join(v["problems"]), "; ".join(v["warnings"])])


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("dir")
    ap.add_argument("--x", help="factor to put on the rows (default: the plan's first factor)")
    ap.add_argument("--include-invalid", action="store_true",
                    help="include runs that failed a validity gate (they are still listed)")
    args = ap.parse_args()

    runs = load_runs(os.path.join(args.dir, "runs.jsonl"))
    if not runs:
        sys.exit("no runs recorded")

    meta = {}
    meta_path = os.path.join(args.dir, "meta.json")
    if os.path.exists(meta_path):
        with open(meta_path) as f:
            meta = json.load(f)
    plan_fixed = (meta.get("plan") or {}).get("fixed")
    plan_factors = list(((meta.get("plan") or {}).get("factors") or {}))

    variants = [v for v in planlib.VARIANTS if any(r["variant"] == v for r in runs)]
    cells, verdicts = group(runs, args.include_invalid, plan_fixed)
    x = args.x or (plan_factors[0] if plan_factors else None)
    if x is None:
        sys.exit("the plan has no factors; nothing to tabulate")

    report = render(meta, cells, verdicts, variants, x, args.include_invalid)
    path = os.path.join(args.dir, "report.md")
    with open(path, "w") as f:
        f.write(report)
    write_csvs(args.dir, cells, verdicts, variants)
    print(path)


if __name__ == "__main__":
    main()
