#!/usr/bin/env python3
"""Drive recovery_bench (crud_perf_exec/src/bin/recovery_bench.rs).

    run_recovery.py plan PLAN.yml
    run_recovery.py run  PLAN.yml [--out DIR] [--resume] [--binary PATH]

One process per (executor, state size, value size, pre-trigger wait); other axes are comma lists
swept inside it. Processes run in seeded random order, under `taskset` if the plan sets `cpus`."""

import argparse
import itertools
import json
import os
import random
import shutil
import subprocess
import sys
import time

import yaml

SUITE_DIR = os.path.dirname(os.path.abspath(__file__))
CRUD_PERF = os.path.normpath(os.path.join(SUITE_DIR, ".."))
DEFAULT_BINARY = os.path.join(CRUD_PERF, "crud_perf_exec", "target", "release", "recovery_bench")

EXECUTORS = ("dual_state", "crud_single", "crud_scalable")
# Swept inside one process, passed as comma lists
LIST_ARGS = ("depths", "positions", "backlogs", "batch_sizes", "op_cost_us")
# Passed as a single value
SCALAR_ARGS = ("write_pct", "reproposal", "trials", "timeout_s", "seed")


def load(path):
    with open(path) as f:
        plan = yaml.safe_load(f)
    for key in ("name", "executors", "process_factors", "bench_args"):
        if key not in plan:
            raise SystemExit(f"plan is missing '{key}'")
    plan.setdefault("scenario", "recovery")
    # `primitives` starts no executor; it is the placeholder the benchmark expects.
    allowed = ("primitives",) if plan["scenario"] == "primitives" else EXECUTORS
    for e in plan["executors"]:
        if e not in allowed:
            raise SystemExit(f"unknown executor {e!r}; expected one of {allowed}")
    plan.setdefault("seed", 0)
    return plan


def units(plan):
    """Every process to launch, in a seeded random order."""
    pf = plan["process_factors"]
    names = list(pf)
    out = []
    for executor in plan["executors"]:
        for combo in itertools.product(*[pf[n] for n in names]):
            out.append((executor, dict(zip(names, combo))))
    random.Random(plan["seed"]).shuffle(out)
    return out


def unit_id(plan, executor, factors):
    tail = "_".join(f"{k}{v}" for k, v in sorted(factors.items()))
    return f"{plan['name']}_{executor}_{tail}"


def command(plan, executor, factors, binary, out_path):
    ba = plan["bench_args"]
    cmd = [binary, "--executor", executor, "--scenario", plan["scenario"], "--out", out_path]

    # process-level factors map straight onto flags
    for name, value in factors.items():
        cmd += [f"--{name.replace('_', '-')}", str(value)]

    for name in LIST_ARGS:
        if name in ba:
            vals = ba[name] if isinstance(ba[name], list) else [ba[name]]
            cmd += [f"--{name.replace('_', '-')}", ",".join(str(v) for v in vals)]
    for name in SCALAR_ARGS:
        if name in ba:
            cmd += [f"--{name.replace('_', '-')}", str(ba[name])]

    cpus = plan.get("cpus")
    if cpus and shutil.which("taskset"):
        cmd = ["taskset", "-c", str(cpus)] + cmd
    return cmd


def cmd_plan(args):
    plan = load(args.plan)
    us = units(plan)
    ba = plan["bench_args"]
    configs = 1
    for name in ("depths", "positions", "backlogs", "batch_sizes", "op_cost_us"):
        v = ba.get(name, [1])
        configs *= len(v) if isinstance(v, list) else 1
    trials = int(ba.get("trials", 30))
    print(f"plan       {plan['name']}   scenario={plan['scenario']}")
    print(f"processes  {len(us)}   ({len(plan['executors'])} executors x "
          f"{len(us) // len(plan['executors'])} process-level cells)")
    print(f"per process: up to {configs} configurations x {trials} trials")
    print(f"binary     {args.binary or DEFAULT_BINARY}  "
          f"({'present' if os.path.exists(args.binary or DEFAULT_BINARY) else 'MISSING -- cargo build --release --bin recovery_bench'})")
    if plan.get("cpus"):
        print(f"pinned to  cpus {plan['cpus']}  (taskset {'found' if shutil.which('taskset') else 'MISSING'})")
    print("\nfirst processes:")
    for ex, f in us[:8]:
        print(f"  {ex:14s} {f}")
    return 0


def cmd_run(args):
    plan = load(args.plan)
    binary = args.binary or DEFAULT_BINARY
    if not os.path.exists(binary):
        sys.exit(f"{binary} not found; build it: (cd {CRUD_PERF}/crud_perf_exec && "
                 "cargo build --release --bin recovery_bench)")

    out_dir = os.path.abspath(args.out or os.path.join(SUITE_DIR, "results", plan["name"]))
    os.makedirs(os.path.join(out_dir, "raw"), exist_ok=True)
    results = os.path.join(out_dir, "results.jsonl")
    done_path = os.path.join(out_dir, "done.txt")
    done = set(open(done_path).read().split()) if os.path.exists(done_path) else set()
    if os.path.exists(results) and not args.resume and not done:
        sys.exit(f"{results} already exists; pass --resume to continue or choose another --out")

    meta = os.path.join(out_dir, "meta.json")
    if not os.path.exists(meta):
        with open(meta, "w") as f:
            json.dump({"plan": plan, "binary": binary,
                       "host_cpus": os.cpu_count(), "kernel": os.uname().release,
                       "started": time.strftime("%Y-%m-%dT%H:%M:%S%z")}, f, indent=2, default=str)

    todo = [(e, f) for e, f in units(plan) if unit_id(plan, e, f) not in done]
    print(f"{len(todo)} processes to run ({len(done)} already done) -> {out_dir}", flush=True)

    for n, (executor, factors) in enumerate(todo, 1):
        uid = unit_id(plan, executor, factors)
        cmd = command(plan, executor, factors, binary, results)
        print(f"[{n}/{len(todo)}] {uid}", flush=True)
        t0 = time.time()
        with open(os.path.join(out_dir, "raw", uid + ".stderr.log"), "w") as err:
            try:
                p = subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=err,
                                   timeout=int(plan.get("process_timeout_s", 7200)))
                status = f"exit {p.returncode}"
            except subprocess.TimeoutExpired:
                status = "TIMED OUT"
        print(f"    {status} in {time.time() - t0:.0f}s", flush=True)
        with open(done_path, "a") as f:
            f.write(uid + "\n")
    print("done")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("plan", "run"):
        p = sub.add_parser(name)
        p.add_argument("plan")
        p.add_argument("--binary")
    sub.choices["run"].add_argument("--out")
    sub.choices["run"].add_argument("--resume", action="store_true")
    args = ap.parse_args()
    sys.exit({"plan": cmd_plan, "run": cmd_run}[args.cmd](args))


if __name__ == "__main__":
    main()
