"""Experiment plans: loading, validation, expansion into ordered runs.

`factors` are crossed in full; `fixed` is passed to make as overrides. Runs are ordered as
randomised blocks (one per cell x repetition, all variants back to back in shuffled order, blocks
shuffled), seeded so an interrupted sweep resumes identically."""

import hashlib
import itertools
import json
import os
import random
import re

import yaml

from . import wan

VARIANTS = ("baseline", "dual_state", "crud_single", "crud_scalable")
SPECULATIVE_VARIANTS = ("dual_state", "crud_single", "crud_scalable")

# Factors with a meaning of their own; every other factor must be an upper-case make variable.
SPECIAL_FACTORS = ("rtt_ms", "op_cost_us")

REQUIRED_FIXED = ("N_REPLICAS", "N_CLIENTS", "CONCURRENT_RQS", "WARMUP_SECS", "MEASURE_SECS")


class PlanError(Exception):
    pass


def _resolve(path):
    """Read a plan file and fold in whatever it `extends`. Not validated: a base file is
    deliberately incomplete (it has no name), and only the merged result has to be whole."""
    path = os.path.abspath(path)
    with open(path) as f:
        plan = yaml.safe_load(f) or {}

    parent = plan.pop("extends", None)
    if parent:
        base = _resolve(os.path.join(os.path.dirname(path), parent))
        merged = dict(base)
        merged.update({k: v for k, v in plan.items() if k != "fixed"})
        merged["fixed"] = {**base.get("fixed", {}), **plan.get("fixed", {})}
        plan = merged

    plan["_path"] = path
    return plan


def load(path):
    plan = _resolve(path)
    validate(plan)
    return plan


def validate(plan):
    for key in ("name", "variants", "fixed"):
        if key not in plan:
            raise PlanError(f"plan is missing '{key}'")

    if not re.fullmatch(r"[A-Za-z0-9_]+", plan["name"]):
        raise PlanError(
            f"plan name {plan['name']!r} must be [A-Za-z0-9_]+: it becomes an InfluxDB tag")

    for v in plan["variants"]:
        if v not in VARIANTS:
            raise PlanError(f"unknown variant {v!r}; expected one of {VARIANTS}")
    if len(set(plan["variants"])) != len(plan["variants"]):
        raise PlanError("duplicate variant")

    for key in REQUIRED_FIXED:
        if key not in plan["fixed"]:
            raise PlanError(f"fixed.{key} must be set explicitly")

    if int(plan["fixed"]["MEASURE_SECS"]) <= 0:
        raise PlanError(
            "fixed.MEASURE_SECS must be > 0: the suite uses time-based phases so that runs "
            "of very different throughput get equally long measurement windows")

    for name, levels in (plan.get("factors") or {}).items():
        if name not in SPECIAL_FACTORS and not re.fullmatch(r"[A-Z][A-Z0-9_]*", name):
            raise PlanError(
                f"factor {name!r}: use one of {SPECIAL_FACTORS} or an UPPER_CASE make variable")
        if not isinstance(levels, list) or not levels:
            raise PlanError(f"factor {name!r} needs a non-empty list of levels")

    if "rtt_ms" in (plan.get("factors") or {}) and str(plan["fixed"].get("WAN_ENABLED", "0")) != "1":
        raise PlanError("an rtt_ms factor needs fixed.WAN_ENABLED: 1")


def fingerprint(plan):
    """Stable hash of everything that determines what the runs are. Stored with the results so
    that --resume refuses to continue a sweep under an edited plan."""
    material = {k: v for k, v in plan.items() if not k.startswith("_")}
    return hashlib.sha256(json.dumps(material, sort_keys=True, default=str).encode()).hexdigest()[:12]


class Run:
    def __init__(self, idx, rep, factors, variant, plan):
        self.idx = idx
        self.rep = rep
        self.factors = factors
        self.variant = variant
        self.plan = plan

    @property
    def run_id(self):
        return f"{self.plan['name']}_{self.idx:04d}_{self.variant}"

    @property
    def cell_key(self):
        return tuple(sorted(self.factors.items()))

    def make_args(self):
        """The `VAR=value` overrides for `make crud_perf local`. Command-line, not environment: a variable
        in an included bench.env outranks the environment."""
        args = {str(k): str(v) for k, v in self.plan["fixed"].items()}
        args["EXECUTOR_VARIANT"] = self.variant
        args["INFLUX_EXTRA"] = self.run_id

        for name, value in self.factors.items():
            if name == "rtt_ms":
                args["WAN_ENABLED"] = "1"
                args["WAN_PROFILE"] = wan.profile_name(value)
            elif name == "op_cost_us":
                # Both units: with _US at 0 the binary falls back to _MS (default 10ms).
                args["EXPENSIVE_OP_SLEEP_US"] = str(int(value))
                args["EXPENSIVE_OP_SLEEP_MS"] = "0"
                args["WORKLOAD_TYPE"] = "uniform_expensive" if value > 0 else "uniform_cheap"
            else:
                args[name] = str(value)
        return args


def expand(plan, quick=False):
    factors = plan.get("factors") or {}
    names = list(factors)
    levels = [factors[n] for n in names]

    reps = int(plan.get("repetitions", 1))
    variants = list(plan["variants"])

    if quick:
        # Smoke-test shape: first and last level of each factor, a single repetition.
        levels = [[lv[0], lv[-1]] if len(lv) > 2 else lv for lv in levels]
        reps = 1

    cells = list(itertools.product(*levels)) if names else [()]
    blocks = [(cell, rep) for rep in range(reps) for cell in cells]

    rng = random.Random(plan.get("seed", 0))
    if plan.get("order", "randomized-blocks") == "randomized-blocks":
        rng.shuffle(blocks)

    runs = []
    for cell, rep in blocks:
        order = variants[:]
        if plan.get("order", "randomized-blocks") == "randomized-blocks":
            rng.shuffle(order)
        for variant in order:
            runs.append(Run(len(runs), rep, dict(zip(names, cell)), variant, plan))
    return runs


def estimate_seconds(plan, runs, overhead_s=75):
    """Rough wall-clock for the whole sweep. `overhead_s` is boot + bootstrap + drain +
    teardown per run, which dominates short measurement windows."""
    fixed = plan["fixed"]
    per_run = int(fixed["WARMUP_SECS"]) + int(fixed["MEASURE_SECS"]) + overhead_s
    return per_run * len(runs)
