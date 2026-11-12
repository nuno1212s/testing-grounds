"""Tests for the recovery driver and analysis. Run: python3 -m unittest discover -s tests"""

import json
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import analyze_recovery as ar  # noqa: E402
import run_recovery as rr  # noqa: E402


def row(executor, keys, ttr, *, trial=5, status="ok", depth=32, position="head", backlog=0,
        batch=100, cost=0, wait=0, value=64, scenario="recovery", **extra):
    r = {"executor": executor, "scenario": scenario, "state_keys": keys, "value_bytes": value,
         "write_pct": 50, "pre_trigger_wait_ms": wait, "depth": depth, "position": position,
         "backlog": backlog, "batch_size": batch, "op_cost_us": cost, "reproposal": "one",
         "trial": trial, "status": status}
    if status == "ok" and scenario == "recovery":
        r.update(ttr_first_ms=ttr, ttr_full_ms=ttr, mismatches=0, stale_replies=0)
    r.update(extra)
    return r


def dataset():
    """dual_state's recovery is a floor plus a term proportional to state; the CRUD executors'
    is flat. The analysis has to recover exactly that."""
    rows = []
    for keys in (1_000, 10_000, 100_000, 1_000_000):
        for trial in range(2, 12):
            jitter = 1 + 0.01 * (trial % 3)
            rows.append(row("dual_state", keys, (0.05 + keys * 64 / 1e6 * 0.5) * jitter, trial=trial))
            rows.append(row("crud_single", keys, 0.05 * jitter, trial=trial))
            rows.append(row("crud_scalable", keys, 0.07 * jitter, trial=trial))
    return rows


class SummaryTests(unittest.TestCase):
    def test_cold_trials_are_discarded(self):
        rows = [row("crud_single", 1000, 99.0, trial=0), row("crud_single", 1000, 99.0, trial=1)]
        rows += [row("crud_single", 1000, 1.0, trial=t) for t in range(2, 8)]
        st = next(iter(ar.summarise(rows, discard=2).values()))
        self.assertEqual(st["n_ok"], 6)
        self.assertAlmostEqual(st["ttr_first_ms"]["median"], 1.0)
        self.assertAlmostEqual(st["ttr_first_ms"]["max"], 1.0)

    def test_wedges_and_wrong_answers_are_counted_not_averaged(self):
        rows = [row("dual_state", 1000, 1.0, trial=t) for t in range(2, 6)]
        rows += [row("dual_state", 1000, None, trial=6, status="timeout"),
                 row("dual_state", 1000, 9999.0, trial=7, status="mismatch")]
        st = next(iter(ar.summarise(rows, discard=2).values()))
        self.assertEqual((st["n_ok"], st["n_timeout"], st["n_mismatch"]), (4, 1, 1))
        self.assertAlmostEqual(st["ttr_first_ms"]["max"], 1.0)   # the 9999 never reached a time

    def test_percentile_interpolation(self):
        self.assertEqual(ar.pct([1, 2, 3, 4, 5], 50), 3)
        self.assertAlmostEqual(ar.pct([0, 10], 25), 2.5)
        self.assertIsNone(ar.pct([], 50))


class ReportTests(unittest.TestCase):
    def render(self, rows):
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "results.jsonl"), "w") as f:
                for r in rows:
                    f.write(json.dumps(r) + "\n")
            sys.argv = ["analyze_recovery.py", d]
            ar.main()
            return open(os.path.join(d, "report.md")).read(), open(os.path.join(d, "summary.csv")).read()

    def test_scaling_exponents_recover_the_construction(self):
        stats = ar.summarise(dataset(), discard=2)
        table = ar.scaling_table(stats)
        line = next(l for l in table.splitlines() if "position=head" in l)
        cells = [c.strip() for c in line.strip("|").split("|")]
        dual_all, dual_large = [float(x) for x in cells[1].split("/")]
        crud_all, crud_large = [float(x) for x in cells[2].split("/")]
        self.assertGreater(dual_large, 0.9)    # state-proportional once the state dominates
        self.assertLess(dual_all, dual_large)  # the small end is flattened by the fixed floor
        self.assertAlmostEqual(crud_all, 0.0, places=1)
        self.assertAlmostEqual(crud_large, 0.0, places=1)

    def test_report_ratio_and_structure(self):
        report, csv_text = self.render(dataset())
        self.assertIn("## Recovery time against state size", report)
        self.assertIn("dual_state ÷ crud_single", report)
        self.assertIn("1,000,000", report)
        self.assertEqual(len(csv_text.splitlines()), 1 + 12)  # 4 sizes x 3 executors

    def test_wedge_matrix_shows_where_the_executor_dies(self):
        rows = []
        for backlog, wait in ((0, 0), (16, 0), (16, 50)):
            for ex in ar.EXECUTORS:
                for t in range(6):
                    wedged = ex == "dual_state" and backlog == 16 and wait == 0
                    rows.append(row(ex, 1000, None if wedged else 1.0, trial=t, backlog=backlog,
                                    wait=wait, status="timeout" if wedged else "ok"))
        report, _ = self.render(rows)
        self.assertIn("## Wedge rate against confirmed backlog", report)
        self.assertIn("**6/6**", report)
        self.assertEqual(report.count("**6/6**"), 1)   # only that one cell
        self.assertIn("## Failures", report)

    def test_snapshot_table(self):
        rows = []
        for keys, ck in ((1000, 0.1), (100_000, 10.0)):
            for ex, probe in (("dual_state", 0.2), ("crud_single", ck)):
                for t in range(2, 6):
                    rows.append(row(ex, keys, None, trial=t, scenario="snapshot", ckpt_ms=ck, probe_ms=probe,
                                    mismatches=0))
        report, _ = self.render(rows)
        self.assertIn("## Snapshot (checkpoint) cost and stall", report)
        self.assertIn("probe reply ms", report)


class PrimitiveTests(unittest.TestCase):
    def test_primitive_tables_show_clone_growing_and_rebuild_flat(self):
        rows = []
        for keys, clone_ms in ((1000, 0.02), (100_000, 14.0), (1_000_000, 180.0)):
            for t in range(2, 8):
                rows.append({"scenario": "primitives", "executor": "primitives", "what": "state_clone",
                             "state_keys": keys, "value_bytes": 64, "trial": t, "status": "ok", "ms": clone_ms})
                rows.append({"scenario": "primitives", "executor": "primitives", "what": "cache_rebuild",
                             "state_keys": keys, "value_bytes": 64, "trial": t, "status": "ok", "ms": 0.35,
                             "pending_batches": 128, "batch_size": 100, "pending_entries": 6400})
        tables = ar.primitive_tables(rows, discard=2)
        self.assertEqual(len(tables), 2)
        self.assertIn("1,000,000", tables[0])
        self.assertIn("180", tables[0])
        self.assertIn("6,400", tables[1])
        self.assertEqual(tables[1].count("0.35 (0.35–0.35)"), 3)   # one row per state size, same cost

    def test_primitives_plan_loads_with_the_placeholder_executor(self):
        plan = rr.load(os.path.join(os.path.dirname(HERE), "plans", "recovery", "r6_primitives.yml"))
        self.assertEqual(plan["executors"], ["primitives"])
        ex, factors = rr.units(plan)[0]
        cmd = rr.command(plan, ex, factors, "/b", "/o")
        self.assertEqual(cmd[cmd.index("--scenario") + 1], "primitives")

    def test_primitives_plan_rejects_a_real_executor(self):
        with tempfile.NamedTemporaryFile("w", suffix=".yml", delete=False) as f:
            f.write("name: x\nscenario: primitives\nexecutors: [dual_state]\nprocess_factors: {state_keys: [1]}\nbench_args: {}\n")
        try:
            with self.assertRaises(SystemExit):
                rr.load(f.name)
        finally:
            os.unlink(f.name)


class DriverTests(unittest.TestCase):
    PLAN = os.path.join(os.path.dirname(HERE), "plans", "recovery")

    def test_every_shipped_plan_loads_and_builds_commands(self):
        for fn in sorted(os.listdir(self.PLAN)):
            plan = rr.load(os.path.join(self.PLAN, fn))
            us = rr.units(plan)
            self.assertGreater(len(us), 0)
            ex, factors = us[0]
            cmd = rr.command(plan, ex, factors, "/bin/bench", "/tmp/out.jsonl")
            self.assertIn("--executor", cmd)
            self.assertIn(ex, cmd)
            self.assertIn("--scenario", cmd)

    def test_units_are_deterministic_and_complete(self):
        plan = rr.load(os.path.join(self.PLAN, "r1_state_size.yml"))
        a = [(e, tuple(sorted(f.items()))) for e, f in rr.units(plan)]
        b = [(e, tuple(sorted(f.items()))) for e, f in rr.units(plan)]
        self.assertEqual(a, b)
        self.assertEqual(len(a), 3 * 4)
        self.assertEqual(len(set(a)), len(a))

    def test_lists_become_comma_separated_flags(self):
        plan = rr.load(os.path.join(self.PLAN, "r1_state_size.yml"))
        cmd = rr.command(plan, "dual_state", {"state_keys": 1000, "value_bytes": 64}, "/b", "/o")
        self.assertEqual(cmd[cmd.index("--depths") + 1], "8,32,128")
        self.assertEqual(cmd[cmd.index("--positions") + 1], "head,tail")
        self.assertEqual(cmd[cmd.index("--state-keys") + 1], "1000")
        self.assertEqual(cmd[cmd.index("--value-bytes") + 1], "64")

    def test_pre_trigger_wait_is_a_process_level_flag(self):
        plan = rr.load(os.path.join(self.PLAN, "r2_confirmed_backlog.yml"))
        cmd = rr.command(plan, "dual_state",
                         {"state_keys": 10000, "value_bytes": 64, "pre_trigger_wait_ms": 10}, "/b", "/o")
        self.assertEqual(cmd[cmd.index("--pre-trigger-wait-ms") + 1], "10")


if __name__ == "__main__":
    unittest.main()
