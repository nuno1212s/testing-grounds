"""Tests for the suite's logic. Run: python3 -m unittest discover -s tests   (from suite/)

Everything here uses synthetic data with known answers; nothing needs Docker.
"""

import datetime as dt
import json
import math
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import analyze_suite as an  # noqa: E402
import run_suite as rs  # noqa: E402
from lib import plan as planlib  # noqa: E402
from lib import stats, wan  # noqa: E402
from lib.hist import Histogram, bucket_index, bucket_lower  # noqa: E402

# Shared with histogram.rs::golden_vectors_shared_with_the_python_decoder.
GOLDEN = [
    (0, 0, 0), (1, 1, 1), (63, 63, 63), (64, 64, 64), (65, 65, 65), (127, 127, 127),
    (128, 128, 128), (129, 128, 128), (130, 129, 130), (255, 191, 254), (256, 192, 256),
    (257, 192, 256), (1000, 317, 1000), (65535, 703, 65024), (65536, 704, 65536),
    (1000000, 954, 999424), (123456789, 1397, 122683392), (1099511627776, 2240, 1099511627776),
    (4611686018427387904, 3648, 4611686018427387904),
    (9223372036854775808, 3712, 9223372036854775808),
    (18446744073709551615, 3775, 18302628885633695744),
]


def hist_json(samples_ns):
    buckets = {}
    for v in samples_ns:
        buckets[bucket_index(v)] = buckets.get(bucket_index(v), 0) + 1
    return {"count": len(samples_ns), "sum_ns": sum(samples_ns),
            "max_ns": max(samples_ns, default=0),
            "buckets": {str(k): v for k, v in buckets.items()}}


class HistogramTests(unittest.TestCase):
    def test_golden_vectors_match_the_rust_encoder(self):
        for value, index, lower in GOLDEN:
            self.assertEqual(bucket_index(value), index, value)
            self.assertEqual(bucket_lower(index), lower, value)

    def test_percentiles_are_within_the_error_bound(self):
        samples = [int(1e6 * (1 + i / 1000)) for i in range(10_000)]  # 1.0 .. 11.0 ms
        h = Histogram.from_json(hist_json(samples))
        for p in (50, 90, 99, 99.9):
            exact = sorted(samples)[math.ceil(p / 100 * len(samples)) - 1]
            self.assertLess(abs(h.percentile_ns(p) - exact) / exact, 1 / 64, p)
        self.assertEqual(h.percentile_ns(100), max(samples))

    def test_merge_is_exact_unlike_averaging_percentiles(self):
        fast, slow = [1_000_000] * 900, [100_000_000] * 100
        merged = Histogram.from_json(hist_json(fast)).merge(Histogram.from_json(hist_json(slow)))
        # 90% of the pooled samples are fast, so p90 is fast and p91 is slow. The mean of the
        # two clients' p50s (1ms and 100ms) would be ~50ms: wrong by 50x.
        self.assertLess(merged.percentile_ns(90), 1.1e6)
        self.assertGreater(merged.percentile_ns(91), 90e6)
        self.assertEqual(merged.count, 1000)

    def test_inconsistent_histogram_is_rejected(self):
        bad = hist_json([1000, 2000])
        bad["count"] = 5
        with self.assertRaises(ValueError):
            Histogram.from_json(bad)


class StatsTests(unittest.TestCase):
    def test_small_sample_ci_uses_student_t(self):
        mean, half, n = stats.mean_ci([1.0, 2.0, 3.0])
        self.assertEqual((mean, n), (2.0, 3))
        # t(2) = 4.303, s = 1, se = 1/sqrt(3)
        self.assertAlmostEqual(half, 4.303 / math.sqrt(3), places=3)

    def test_single_sample_has_no_interval(self):
        self.assertEqual(stats.mean_ci([5.0]), (5.0, None, 1))

    def test_paired_difference_cancels_shared_drift(self):
        drift = [0, 50, 100, 150]                    # slow host drift shared within a block
        baseline = [100 + d for d in drift]
        variant = [90 + d for d in drift]            # a constant 10 ms win under that drift
        mean, half, n = stats.paired_diff(baseline, variant)
        self.assertAlmostEqual(mean, 10.0)
        self.assertAlmostEqual(half, 0.0, places=9)  # the drift is gone from the difference
        # Independent intervals on the same data are enormous by comparison.
        self.assertGreater(stats.mean_ci(baseline)[1], 50)

    def test_loglog_slope_recovers_the_exponent(self):
        self.assertAlmostEqual(stats.loglog_slope([(10 ** i, 3 * 10 ** i) for i in range(1, 7)]), 1.0)
        self.assertAlmostEqual(stats.loglog_slope([(10 ** i, 7.0) for i in range(1, 7)]), 0.0)
        self.assertAlmostEqual(stats.loglog_slope([(10 ** i, 10 ** (2 * i)) for i in range(1, 5)]), 2.0)


class PlanTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.plan = planlib.load(os.path.join(os.path.dirname(HERE), "plans", "e1_latency_processing.yml"))

    def test_expansion_is_deterministic(self):
        a = [(r.run_id, r.variant, tuple(r.factors.items())) for r in planlib.expand(self.plan)]
        b = [(r.run_id, r.variant, tuple(r.factors.items())) for r in planlib.expand(self.plan)]
        self.assertEqual(a, b)

    def test_every_block_runs_all_variants_back_to_back(self):
        runs = planlib.expand(self.plan)
        nv = len(self.plan["variants"])
        self.assertEqual(len(runs), 6 * 5 * 3 * nv)
        for i in range(0, len(runs), nv):
            block = runs[i:i + nv]
            self.assertEqual({r.variant for r in block}, set(self.plan["variants"]))
            self.assertEqual(len({(r.cell_key, r.rep) for r in block}), 1)

    def test_variant_order_is_not_constant(self):
        runs = planlib.expand(self.plan)
        nv = len(self.plan["variants"])
        firsts = {runs[i].variant for i in range(0, len(runs), nv)}
        self.assertGreater(len(firsts), 1, "one variant always runs first: a systematic bias")

    def test_each_cell_gets_the_requested_repetitions(self):
        counts = {}
        for r in planlib.expand(self.plan):
            counts.setdefault((r.cell_key, r.variant), set()).add(r.rep)
        self.assertTrue(all(c == {0, 1, 2} for c in counts.values()))

    def test_quick_is_a_corner_sample_with_one_repetition(self):
        runs = planlib.expand(self.plan, quick=True)
        self.assertEqual(len(runs), 2 * 2 * 4)
        self.assertEqual({r.rep for r in runs}, {0})

    def test_make_args_translate_special_factors(self):
        run = next(r for r in planlib.expand(self.plan)
                   if r.factors == {"rtt_ms": 50, "op_cost_us": 1000})
        args = run.make_args()
        self.assertEqual(args["WAN_PROFILE"], wan.profile_name(50))
        self.assertEqual(args["EXPENSIVE_OP_SLEEP_US"], "1000")
        self.assertEqual(args["EXPENSIVE_OP_SLEEP_MS"], "0")
        self.assertEqual(args["EXECUTOR_VARIANT"], run.variant)
        self.assertEqual(args["INFLUX_EXTRA"], run.run_id)

    def test_zero_cost_really_means_zero_not_the_10ms_default(self):
        run = next(r for r in planlib.expand(self.plan) if r.factors["op_cost_us"] == 0)
        args = run.make_args()
        self.assertEqual((args["EXPENSIVE_OP_SLEEP_US"], args["EXPENSIVE_OP_SLEEP_MS"]), ("0", "0"))
        self.assertEqual(args["WORKLOAD_TYPE"], "uniform_cheap")

    def test_validation_rejects_bad_plans(self):
        good = {"name": "x", "variants": ["baseline"],
                "fixed": {"N_REPLICAS": 4, "N_CLIENTS": 1, "CONCURRENT_RQS": 1,
                          "WARMUP_SECS": 1, "MEASURE_SECS": 1}}
        planlib.validate(good)
        for mutate, why in [
            (lambda p: p["fixed"].update(MEASURE_SECS=0), "time-based phases required"),
            (lambda p: p.update(variants=["nope"]), "unknown variant"),
            (lambda p: p.update(name="has space"), "tag-unsafe name"),
            (lambda p: p.update(factors={"lowercase_thing": [1]}), "unknown lower-case factor"),
            (lambda p: p.update(factors={"rtt_ms": [1]}), "rtt_ms needs WAN_ENABLED"),
        ]:
            bad = json.loads(json.dumps(good))
            mutate(bad)
            with self.assertRaises(planlib.PlanError, msg=why):
                planlib.validate(bad)

    def test_all_shipped_plans_load_and_every_wan_profile_exists(self):
        plans_dir = os.path.join(os.path.dirname(HERE), "plans")
        bench = os.path.join(os.path.dirname(HERE), "..", "..", "bench", "wan-profiles")
        for fn in sorted(os.listdir(plans_dir)):
            if fn.startswith("_") or not fn.endswith(".yml"):
                continue
            plan = planlib.load(os.path.join(plans_dir, fn))
            for r in planlib.expand(plan):
                prof = r.make_args().get("WAN_PROFILE")
                if prof:
                    self.assertTrue(os.path.isfile(os.path.join(bench, prof + ".yml")),
                                    f"{fn}: missing profile {prof}")


class RunnerParsingTests(unittest.TestCase):
    def wrap(self, payload, ts="2026-10-31T12:00:50.000000Z"):
        return json.dumps({"timestamp": ts, "level": "WARN",
                           "fields": {"message": "RESULT " + json.dumps(payload)}})

    def result(self, ordered, client=1000, ops=None, tput=100.0, ts="2026-10-31T12:00:50.000000Z"):
        return {"schema": 1, "client": client, "mode": "duration", "warmup_s": 5.0, "measure_s": 30.0,
                "elapsed_s": 32.0, "ops": ops if ops is not None else len(ordered),
                "throughput": tput, "ordered": hist_json(ordered), "unordered": hist_json([]),
                "_ts": ts}

    def test_results_are_found_in_noisy_container_output(self):
        r = self.result([1_000_000, 2_000_000])
        r.pop("_ts")
        text = "\n".join([
            "not json at all",
            json.dumps({"timestamp": "t", "fields": {"message": "something else"}}),
            self.wrap(r),
            '{"truncated": ',
            json.dumps({"timestamp": "t", "fields": {"message": "RESULT {broken"}}),
        ])
        found = rs.parse_results(text)
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0]["ops"], 2)
        self.assertEqual(found[0]["_ts"], "2026-10-31T12:00:50.000000Z")

    def test_aggregate_pools_clients_and_sums_throughput(self):
        a = self.result([1_000_000] * 100, tput=50.0)
        b = self.result([3_000_000] * 100, client=1001, tput=70.0)
        agg = rs.aggregate([a, b])
        self.assertEqual((agg["clients"], agg["ops"], agg["throughput"]), (2, 200, 120.0))
        self.assertEqual(agg["ordered"]["count"], 200)
        self.assertIsNone(agg["unordered"])
        self.assertAlmostEqual(agg["ordered"]["mean_ms"], 2.0)

    def test_measurement_window_is_located_from_the_drain_stamp(self):
        t0, t1 = rs.measurement_window([self.result([1000])])
        # stamped 12:00:50, 32 s after the window began => began 12:00:18; lasted 30 s
        self.assertTrue(t0.startswith("2026-10-31T12:00:18"), t0)
        self.assertTrue(t1.startswith("2026-10-31T12:00:48"), t1)

    def test_phase_markers_are_found_per_client(self):
        def line(msg):
            return json.dumps({"timestamp": "t", "level": "WARN", "fields": {"message": msg}})
        text = "\n".join([
            line("PHASE warmup_start client=1000"), line("PHASE warmup_start client=1001"),
            line("PHASE measure_start client=1000"), line("something else"),
            line("PHASE warmup_start client=1000"),          # a repeat does not double-count
        ])
        seen = rs.parse_phases(text)
        self.assertEqual(seen["warmup_start"], {1000, 1001})
        self.assertEqual(seen["measure_start"], {1000})
        self.assertEqual(rs.parse_phases("no markers here"), {"warmup_start": set(), "measure_start": set()})

    def test_ping_parsing(self):
        out = "8 packets transmitted, 8 received\nrtt min/avg/max/mdev = 49.8/50.3/51.0/0.4 ms\n"
        self.assertEqual(rs.parse_ping_avg_ms(out), 50.3)
        self.assertIsNone(rs.parse_ping_avg_ms("garbage"))


def synth_run(idx, rep, variant, rtt, cost, p50, *, hit=1000, backtracks=0, restarts=0,
              batch=4.0, n=5000, rtt_measured=None, status="ok", tput=100.0, clients=8, image=None):
    samples = [int(p50 * 1e6)] * n
    return {
        "run_id": f"t_{idx:04d}_{variant}", "plan": "t", "idx": idx, "rep": rep, "variant": variant,
        "factors": {"rtt_ms": rtt, "op_cost_us": cost}, "status": status,
        "agg": {"clients": clients, "ops": n, "throughput": tput,
                "ordered": Histogram.from_json(hist_json(samples)).summary_ms(), "unordered": None},
        "images": {"replica-0": image or f"crud-perf-{variant.replace('_', '-')}-wan"},
        "rtt_measured_ms": {"replica-0->replica-1": rtt if rtt_measured is None else rtt_measured},
        "replica_restarts": {"replica-0": restarts},
        "replica_metrics": ({} if variant == "baseline" else {
            "SPECULATION_HIT_RATE": {"value": hit}, "BATCH_SIZE": {"value": batch},
            "CACHE_BACKTRACK_COUNT": {"value": backtracks}}) | {"BATCH_SIZE": {"value": batch}},
    }


FIXED = {"N_CLIENTS": 8, "N_CLIENT_MACHINES": 1}


class ValidityTests(unittest.TestCase):
    def check(self, run):
        return an.validity(run, FIXED)

    def test_clean_runs_pass(self):
        for v in planlib.VARIANTS:
            problems, _ = self.check(synth_run(0, 0, v, 50, 1000, 100))
            self.assertEqual(problems, [], v)

    def test_speculation_that_did_not_engage_is_excluded(self):
        problems, _ = self.check(synth_run(0, 0, "crud_single", 50, 1000, 100, hit=0))
        self.assertTrue(any("did not carry" in p for p in problems), problems)

    def test_backtracks_in_steady_state_are_excluded(self):
        problems, _ = self.check(synth_run(0, 0, "crud_scalable", 50, 1000, 100, backtracks=3))
        self.assertTrue(any("backtracks" in p for p in problems), problems)

    def test_masked_crash_is_excluded(self):
        problems, _ = self.check(synth_run(0, 0, "baseline", 50, 1000, 100, restarts=1))
        self.assertTrue(any("restarted" in p for p in problems), problems)

    def test_wrong_image_for_the_variant_is_excluded(self):
        problems, _ = self.check(synth_run(0, 0, "dual_state", 50, 1000, 100, image="crud-perf-baseline-wan"))
        self.assertTrue(any("image does not match" in p for p in problems), problems)

    def test_replica_rtt_far_from_the_profile_is_excluded(self):
        problems, _ = self.check(synth_run(0, 0, "baseline", 50, 1000, 100, rtt_measured=75))
        self.assertTrue(any("measured replica RTT" in p for p in problems), problems)
        problems, _ = self.check(synth_run(0, 0, "baseline", 50, 1000, 100, rtt_measured=51.5))
        self.assertEqual(problems, [])

    def test_a_busy_executor_is_flagged_but_kept(self):
        # 43 ops/s x 20 ms = 86% of one sequential executor; 4 wide it is 22%
        problems, warnings = self.check(synth_run(0, 0, "baseline", 50, 20000, 300, tput=43.0))
        self.assertEqual(problems, [])
        self.assertTrue(any("busy" in w for w in warnings), warnings)
        _, warnings = self.check(synth_run(0, 0, "crud_scalable", 50, 20000, 300, tput=43.0))
        self.assertFalse(any("busy" in w for w in warnings), warnings)
        _, warnings = self.check(synth_run(0, 0, "baseline", 50, 20000, 300, tput=2.0))
        self.assertFalse(any("busy" in w for w in warnings), warnings)

    def test_a_single_request_in_flight_cannot_queue(self):
        run = synth_run(0, 0, "baseline", 50, 20000, 300, tput=43.0)
        _, warnings = an.validity(run, {"N_CLIENTS": 1, "CONCURRENT_RQS": 1, "N_CLIENT_MACHINES": 1})
        self.assertFalse(any("busy" in w for w in warnings), warnings)

    def test_missing_clients_are_excluded(self):
        problems, _ = self.check(synth_run(0, 0, "baseline", 50, 1000, 100, clients=5))
        self.assertTrue(any("clients reported" in p for p in problems), problems)

    def test_misaligned_client_windows_are_excluded(self):
        run = synth_run(0, 0, "baseline", 50, 1000, 100)
        base = dt.datetime(2026, 11, 2, 12, 0, 50, tzinfo=dt.timezone.utc)

        def client(start_offset):
            end = base + dt.timedelta(seconds=start_offset + 45)
            return {"_ts": end.isoformat(), "elapsed_s": 45.0, "measure_s": 45.0}

        run["clients"] = [client(0.0), client(0.4), client(1.0)]
        self.assertEqual(self.check(run)[0], [])
        run["clients"] = [client(0.0), client(0.4), client(11.0)]
        problems, _ = self.check(run)
        self.assertTrue(any("windows start" in p for p in problems), problems)

    def test_failed_run_is_excluded(self):
        problems, _ = self.check({"run_id": "x", "variant": "baseline", "status": "timeout", "error": "boom"})
        self.assertTrue(problems)


class AnalysisTests(unittest.TestCase):
    def build(self):
        """2 cells x 4 variants x 3 reps. Baseline p50 = 100 + drift; speculative variants
        are exactly 20 / 30 / 40 ms faster than baseline in the same block."""
        runs, idx = [], 0
        wins = {"baseline": 0, "dual_state": 20, "crud_single": 30, "crud_scalable": 40}
        for rtt in (20, 50):
            for rep in range(3):
                drift = 25 * rep
                for v in planlib.VARIANTS:
                    runs.append(synth_run(idx, rep, v, rtt, 1000, 100 + drift - wins[v]))
                    idx += 1
        return runs

    def test_paired_saving_recovers_the_true_effect_through_drift(self):
        cells, _ = an.group(self.build(), include_invalid=False, plan_fixed=FIXED)
        cell = cells[an.cell_key(self.build()[0])]
        for v, win in (("dual_state", 20), ("crud_single", 30), ("crud_scalable", 40)):
            d, half, n = an.paired_saving(cell, v, "p50_ms")
            self.assertAlmostEqual(d, win, delta=win * 0.02)   # histogram resolution
            self.assertEqual(n, 3)
            self.assertLess(half, 2.0)  # tight, although the baseline itself drifts by +-25 ms

    def test_invalid_runs_are_left_out_of_the_statistics(self):
        runs = self.build()
        bad = next(r for r in runs if r["variant"] == "crud_single" and r["rep"] == 1)
        bad["replica_metrics"]["SPECULATION_HIT_RATE"]["value"] = 0
        cells, verdicts = an.group(runs, include_invalid=False, plan_fixed=FIXED)
        self.assertEqual(sum(1 for v in verdicts if v["problems"]), 1)
        cell = cells[an.cell_key(bad)]
        self.assertNotIn(1, cell["crud_single"])
        self.assertEqual(an.paired_saving(cell, "crud_single", "p50_ms")[2], 2)

    def test_model_saturates_at_the_replica_rtt_and_at_the_execution_time(self):
        flat = {"batch_size": 4.0, "rtt_measured_ms": 50.0}
        # 4 ops x 1 ms = 4 ms of execution, well under a 50 ms RTT: hides all of it
        self.assertEqual(an.predicted_saving_ms("crud_single", {"op_cost_us": 1000}, flat), 4.0)
        # 4 ops x 100 ms = 400 ms of execution: capped at the RTT
        self.assertEqual(an.predicted_saving_ms("crud_single", {"op_cost_us": 100_000}, flat), 50.0)
        # the scalable executor runs the batch 4 wide
        self.assertEqual(an.predicted_saving_ms("crud_scalable", {"op_cost_us": 1000}, flat), 1.0)
        # zero cost: nothing to hide
        self.assertEqual(an.predicted_saving_ms("crud_single", {"op_cost_us": 0}, flat), 0.0)

    def test_end_to_end_report_and_csvs(self):
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "runs.jsonl"), "w") as f:
                for r in self.build():
                    f.write(json.dumps(r) + "\n")
            with open(os.path.join(d, "meta.json"), "w") as f:
                json.dump({"plan": {"name": "t", "fixed": FIXED, "factors": {"rtt_ms": [20, 50], "op_cost_us": [1000]}}}, f)
            sys.argv = ["analyze_suite.py", d]
            an.main()
            report = open(os.path.join(d, "report.md")).read()
            self.assertIn("**op_cost_us=1000**", report)
            self.assertIn("saving vs baseline: crud_single", report)
            self.assertIn("mean / model", report)
            rows = open(os.path.join(d, "summary.csv")).read().splitlines()
            self.assertEqual(len(rows), 1 + 2 * 4)
            flat = open(os.path.join(d, "runs_flat.csv")).read().splitlines()
            self.assertEqual(len(flat), 1 + 24)


if __name__ == "__main__":
    unittest.main()
