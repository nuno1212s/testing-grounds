#!/usr/bin/env python3
"""Tables from recovery_bench results.

    analyze_recovery.py DIR [DIR ...] [--discard N]

Medians with 10th-90th percentile (trials are not independent replicates, so no t-intervals);
the first N trials per configuration are dropped as cold. Wedged (`timeout`) and wrong-reply
(`mismatch`) trials are counted, never averaged. Scaling exponent: slope of log(median recovery)
on log(state bytes); ~1 is proportional to state, ~0 independent of it."""

import argparse
import csv
import json
import os
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib.stats import loglog_slope  # noqa: E402

EXECUTORS = ["dual_state", "crud_single", "crud_scalable"]
CONFIG_KEYS = ["scenario", "state_keys", "value_bytes", "write_pct", "pre_trigger_wait_ms",
               "depth", "position", "backlog", "batch_size", "op_cost_us", "reproposal"]
LARGE_KEYS = 100_000


def load(dirs):
    rows = []
    for d in dirs:
        path = os.path.join(d, "results.jsonl") if os.path.isdir(d) else d
        with open(path) as f:
            rows += [json.loads(l) for l in f if l.strip()]
    return rows


def pct(sorted_xs, p):
    if not sorted_xs:
        return None
    k = (len(sorted_xs) - 1) * p / 100
    lo, hi = int(k), min(int(k) + 1, len(sorted_xs) - 1)
    return sorted_xs[lo] + (sorted_xs[hi] - sorted_xs[lo]) * (k - lo)


def summarise(rows, discard):
    """{(config tuple, executor): stats}"""
    groups = defaultdict(list)
    for r in rows:
        key = (tuple(r.get(k) for k in CONFIG_KEYS), r["executor"])
        groups[key].append(r)

    out = {}
    for key, rs in groups.items():
        ok = [r for r in rs if r["status"] == "ok" and r["trial"] >= discard]
        s = {"n_ok": len(ok),
             "n_timeout": sum(r["status"] == "timeout" for r in rs),
             "n_mismatch": sum(r["status"] == "mismatch" for r in rs),
             "n_trials": len(rs)}
        for field in ("ttr_first_ms", "ttr_full_ms", "ckpt_ms", "probe_ms"):
            xs = sorted(r[field] for r in ok if field in r)
            if xs:
                s[field] = {"median": pct(xs, 50), "p10": pct(xs, 10), "p90": pct(xs, 90),
                            "min": xs[0], "max": xs[-1]}
        out[key] = s
    return out


def cfg(config):
    return dict(zip(CONFIG_KEYS, config))


def fmt_time(st, field="ttr_first_ms"):
    if st is None:
        return "-"
    if field not in st:
        bad = []
        if st["n_timeout"]:
            bad.append(f"{st['n_timeout']} wedged")
        if st["n_mismatch"]:
            bad.append(f"{st['n_mismatch']} WRONG")
        return ", ".join(bad) or "-"
    f = st[field]
    cell = f"{f['median']:.3g} ({f['p10']:.3g}–{f['p90']:.3g})"
    if st["n_timeout"]:
        cell += f" ⚠ {st['n_timeout']}/{st['n_trials']} wedged"
    if st["n_mismatch"]:
        cell += f" ⚠ {st['n_mismatch']} WRONG"
    return cell


def md_table(header, rows):
    out = ["| " + " | ".join(header) + " |", "|" + "|".join("---" for _ in header) + "|"]
    out += ["| " + " | ".join(str(c) for c in r) + " |" for r in rows]
    return "\n".join(out)


def slice_label(c, drop=()):
    parts = []
    for k in ("depth", "position", "backlog", "batch_size", "op_cost_us", "pre_trigger_wait_ms",
              "value_bytes", "reproposal", "write_pct"):
        if k in drop or c.get(k) is None:
            continue
        unit = {"op_cost_us": "µs/op", "pre_trigger_wait_ms": "ms wait", "value_bytes": "B values"}.get(k, "")
        parts.append(f"{k}={c[k]}{(' ' + unit) if unit else ''}")
    return ", ".join(parts)


def tables_by_state_size(stats, scenario, field):
    """One table per slice: rows = state size, columns = executors."""
    slices = defaultdict(lambda: defaultdict(dict))   # slice cfg-without-N -> N -> executor -> stats
    for (config, ex), st in stats.items():
        c = cfg(config)
        if c["scenario"] != scenario:
            continue
        rest = tuple(v for k, v in zip(CONFIG_KEYS, config) if k != "state_keys")
        slices[rest][c["state_keys"]][ex] = st

    out = []
    for rest in sorted(slices, key=str):
        c = dict(zip([k for k in CONFIG_KEYS if k != "state_keys"], rest))
        by_n = slices[rest]
        header = ["state keys"] + EXECUTORS + ["dual_state ÷ crud_single"]
        rows = []
        for n in sorted(by_n):
            exs = by_n[n]
            ds, cs = exs.get("dual_state"), exs.get("crud_single")
            ratio = "-"
            if ds and cs and field in ds and field in cs and cs[field]["median"] > 0:
                ratio = f"{ds[field]['median'] / cs[field]['median']:.1f}×"
            rows.append([f"{n:,}"] + [fmt_time(exs.get(e), field) for e in EXECUTORS] + [ratio])
        out.append(f"**{slice_label(c)}**\n\n{md_table(header, rows)}")
    return out


def scaling_table(stats, field="ttr_first_ms"):
    series = defaultdict(dict)   # (slice, executor) -> state_keys -> (state_bytes, median)
    for (config, ex), st in stats.items():
        c = cfg(config)
        if c["scenario"] != "recovery" or field not in st:
            continue
        rest = tuple(v for k, v in zip(CONFIG_KEYS, config) if k != "state_keys")
        series[(rest, ex)][c["state_keys"]] = (c["state_keys"] * c["value_bytes"], st[field]["median"])

    rows = []
    for rest in sorted({k[0] for k in series}, key=str):
        c = dict(zip([k for k in CONFIG_KEYS if k != "state_keys"], rest))
        row = [slice_label(c, drop=("value_bytes",))]
        have = False
        for ex in EXECUTORS:
            pts = list(series.get((rest, ex), {}).items())
            if len({n for n, _ in pts}) < 3:
                row.append("-")
                continue
            have = True
            allp = [v for _, v in pts]
            large = [v for n, v in pts if n >= LARGE_KEYS]
            a = loglog_slope(allp)
            b = loglog_slope(large) if len(large) >= 2 else None
            row.append(f"{a:.2f} / {'-' if b is None else f'{b:.2f}'}")
        if have:
            rows.append(row)
    if not rows:
        return ""
    return md_table(["slice"] + [f"{e}  (all / ≥{LARGE_KEYS:,} keys)" for e in EXECUTORS], rows)


def failure_table(stats):
    rows = []
    for (config, ex), st in sorted(stats.items(), key=lambda kv: str(kv[0])):
        if st["n_timeout"] or st["n_mismatch"]:
            c = cfg(config)
            rows.append([ex, c["scenario"], f"{c['state_keys']:,}", slice_label(c),
                         f"{st['n_timeout']}/{st['n_trials']}", st["n_mismatch"]])
    if not rows:
        return ""
    return md_table(["executor", "scenario", "keys", "configuration", "wedged", "wrong replies"], rows)


def wedge_matrix(stats):
    """R2's headline: wedge rate by executor against backlog and pre-trigger wait."""
    cells = defaultdict(lambda: [0, 0])    # (backlog, wait, cost, executor) -> [wedged, total]
    for (config, ex), st in stats.items():
        c = cfg(config)
        if c["scenario"] != "recovery":
            continue
        key = (c["backlog"], c["pre_trigger_wait_ms"], c["op_cost_us"], ex)
        cells[key][0] += st["n_timeout"]
        cells[key][1] += st["n_trials"]
    keys = sorted({k[:3] for k in cells}, key=str)
    if len({k[0] for k in keys}) < 2:
        return ""
    rows = []
    for backlog, wait, cost in keys:
        row = [backlog, f"{wait} ms" if wait is not None else "0 ms", f"{cost} µs"]
        for ex in EXECUTORS:
            w, t = cells.get((backlog, wait, cost, ex), [0, 0])
            row.append("-" if not t else (f"**{w}/{t}**" if w else f"0/{t}"))
        rows.append(row)
    return md_table(["confirmed backlog", "wait before backtrack", "cost/op"] + EXECUTORS, rows)


def snapshot_tables(stats):
    by_n = defaultdict(dict)
    for (config, ex), st in stats.items():
        c = cfg(config)
        if c["scenario"] == "snapshot":
            by_n[(c["value_bytes"], c["batch_size"], c["op_cost_us"])].setdefault(c["state_keys"], {})[ex] = st
    out = []
    for key, d in sorted(by_n.items()):
        header = (["state keys"] + [f"{e}: checkpoint ms" for e in EXECUTORS]
                  + [f"{e}: probe reply ms" for e in EXECUTORS])
        rows = []
        for n in sorted(d):
            rows.append([f"{n:,}"]
                        + [fmt_time(d[n].get(e), "ckpt_ms") for e in EXECUTORS]
                        + [fmt_time(d[n].get(e), "probe_ms") for e in EXECUTORS])
        out.append(f"**{key[0]} B values, batch {key[1]}, {key[2]} µs/op**\n\n{md_table(header, rows)}")
    return out


def primitive_tables(rows, discard):
    """State::clone against state size, and the cache re-merge against pending entries."""
    prim = [r for r in rows if r.get("scenario") == "primitives" and r.get("status") == "ok"
            and r["trial"] >= discard]
    if not prim:
        return []

    def med(xs):
        xs = sorted(xs)
        return pct(xs, 50), pct(xs, 10), pct(xs, 90)

    out = []

    clone = defaultdict(lambda: defaultdict(list))
    for r in prim:
        if r["what"] in ("state_clone", "state_clone_of_clone"):
            clone[(r["value_bytes"], r["state_keys"])][r["what"]].append(r["ms"])
    if clone:
        rows_ = []
        for (vb, n), kinds in sorted(clone.items()):
            cells = []
            for kind in ("state_clone", "state_clone_of_clone"):
                if kind in kinds:
                    m, lo, hi = med(kinds[kind])
                    cells += [f"{m:.4g} ({lo:.4g}–{hi:.4g})", f"{m * 1e6 / n:,.0f}"]
                else:
                    cells += ["-", "-"]
            rows_.append([f"{n:,}", f"{vb}", f"{n * vb / 1e6:,.1f} MB"] + cells)
        out.append("**`State::clone()`: what `dual_state` pays on every backtrack, and the floor under its recovery**\n\n"
                   "Two columns because the cost depends on how the source state's memory is laid out: a freshly "
                   "built state allocates its values in insertion order, a clone allocates them in hash-iteration "
                   "order, so cloning a clone reads memory sequentially. The dual-state executors' states are "
                   "clones.\n\n"
                   + md_table(["state keys", "value B", "values total", "clone of a built state: ms", "ns/key",
                               "clone of a clone: ms", "ns/key"], rows_))

    rebuild = defaultdict(list)
    for r in prim:
        if r["what"] == "cache_rebuild":
            rebuild[(r["state_keys"], r["pending_batches"], r["batch_size"], r["pending_entries"])].append(r["ms"])
    if rebuild:
        rows_ = []
        for (n, pb, bs, ent), xs in sorted(rebuild.items()):
            m, lo, hi = med(xs)
            rows_.append([f"{n:,}", pb, bs, f"{ent:,}", f"{m:.4g} ({lo:.4g}–{hi:.4g})",
                          f"{m * 1e6 / ent:,.0f}"])
        out.append("**Cache rebuild: re-merging the surviving deltas, what both CRUD executors do on a backtrack**\n\n"
                   "The state size column is there to show it makes no difference: the rebuild reads the pending "
                   "deltas and never the state.\n\n"
                   + md_table(["state keys", "pending batches", "batch size", "pending entries", "ms", "ns per entry"], rows_))
    return out


def write_csv(path, stats):
    with open(path, "w", newline="") as f:
        w = csv.writer(f)
        fields = ["ttr_first_ms", "ttr_full_ms", "ckpt_ms", "probe_ms"]
        header = ["executor"] + CONFIG_KEYS + ["n_trials", "n_ok", "n_timeout", "n_mismatch"]
        for fld in fields:
            header += [f"{fld}_median", f"{fld}_p10", f"{fld}_p90"]
        w.writerow(header)
        for (config, ex), st in sorted(stats.items(), key=lambda kv: str(kv[0])):
            row = [ex] + list(config) + [st["n_trials"], st["n_ok"], st["n_timeout"], st["n_mismatch"]]
            for fld in fields:
                f_ = st.get(fld)
                row += [f_["median"], f_["p10"], f_["p90"]] if f_ else ["", "", ""]
            w.writerow(row)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("dirs", nargs="+")
    ap.add_argument("--discard", type=int, default=2, help="cold trials to drop per configuration")
    args = ap.parse_args()

    rows = load(args.dirs)
    if not rows:
        sys.exit("no results")
    stats = summarise(rows, args.discard)

    L = ["# Backtrack recovery", "",
         f"{len(rows)} trials, {len(stats)} (configuration, executor) cells; first {args.discard} "
         "trials of each configuration discarded as cold. Times in milliseconds: median "
         "(10th–90th percentile). `⚠ k/n wedged` = the executor stopped answering in k of n trials; "
         "`WRONG` = replies disagreed with the reference model.", ""]

    fail = failure_table(stats)
    if fail:
        L += ["## Failures", "", fail, ""]

    wm = wedge_matrix(stats)
    if wm:
        L += ["## Wedge rate against confirmed backlog", "",
              "Trials in which the executor stopped answering after a backtrack that arrived while "
              "confirmed batches were still in flight (wedged / trials).", "", wm, ""]

    first = tables_by_state_size(stats, "recovery", "ttr_first_ms")
    if first:
        L += ["## Recovery time against state size", "",
              "Time from the re-proposal being sent until the corrected reply arrives.", ""] + \
             [t + "\n" for t in first]
        sc = scaling_table(stats)
        if sc:
            L += ["### Scaling exponent against state bytes", "",
                  "Slope of log(median recovery) on log(state bytes). ~1: proportional to the "
                  "state. ~0: independent of it.", "", sc, ""]

    prims = primitive_tables(rows, args.discard)
    if prims:
        L += ["## The two recovery primitives, in isolation", "",
              "Timed on their own, outside any executor. `State::clone` is the real type; the cache "
              "re-merge is a verbatim copy of the (crate-private) algorithm, so treat the end-to-end "
              "recovery times above as authoritative and these as the decomposition.", ""] + \
             [t + "\n" for t in prims]

    snap = snapshot_tables(stats)
    if snap:
        L += ["## Snapshot (checkpoint) cost and stall", "",
              "`checkpoint` = request to snapshot delivered. `probe reply` = a one-operation batch "
              "queued right behind the request; if it waits for the snapshot, taking one stalls "
              "speculation.", ""] + [t + "\n" for t in snap]

    first_dir = args.dirs[0] if os.path.isdir(args.dirs[0]) else os.path.dirname(args.dirs[0])
    path = os.path.join(first_dir, "report.md")
    with open(path, "w") as f:
        f.write("\n".join(L))
    write_csv(os.path.join(first_dir, "summary.csv"), stats)
    print(path)


if __name__ == "__main__":
    main()
