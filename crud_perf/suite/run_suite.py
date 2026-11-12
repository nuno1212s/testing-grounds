#!/usr/bin/env python3
"""Run an experiment plan against the local Docker + WAN-emulation benchmark.

    run_suite.py plan   PLAN.yml [--quick]
    run_suite.py run    PLAN.yml [--quick] [--resume] [--out DIR] [--limit N] [--variants a,b]
    run_suite.py status DIR

A run is `make crud_perf local` with the plan's settings as overrides. The result is the
clients' RESULT histograms; InfluxDB is only used for replica-side validity facts. Fairness lives
in lib/plan.py (block ordering) and bench.env / the compose generator (hardware, requests, state)."""

import argparse
import datetime as dt
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
import tomllib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import plan as planlib  # noqa: E402
from lib import wan  # noqa: E402
from lib.hist import Histogram  # noqa: E402
from lib.influx import Influx  # noqa: E402

SUITE_DIR = os.path.dirname(os.path.abspath(__file__))
TG = os.path.normpath(os.path.join(SUITE_DIR, "..", ".."))
BENCH = os.path.join(TG, "bench")
GENERATED = os.path.join(BENCH, "generated")
ATLAS_ROOT = os.path.normpath(os.path.join(TG, ".."))


class RunFailed(Exception):
    pass


class RunTimeout(Exception):
    pass


# ── parsing and aggregation (pure; unit-tested) ────────────────────────────────────────

def parse_results(console_text):
    """Every client RESULT in a container's console (wrapped in tracing's JSON; the wrapper's
    timestamp is kept)."""
    results = []
    for line in console_text.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            outer = json.loads(line)
        except ValueError:
            continue
        message = (outer.get("fields") or {}).get("message", "")
        if not isinstance(message, str) or not message.startswith("RESULT "):
            continue
        try:
            result = json.loads(message[len("RESULT "):])
        except ValueError:
            continue
        result["_ts"] = outer.get("timestamp")
        results.append(result)
    return results


def parse_phases(console_text):
    """{"warmup_start": {client ids}, "measure_start": {client ids}} announced so far."""
    seen = {"warmup_start": set(), "measure_start": set()}
    for line in console_text.splitlines():
        if "PHASE " not in line:
            continue
        m = re.search(r"PHASE (warmup_start|measure_start) client=(\d+)", line)
        if m:
            seen[m.group(1)].add(int(m.group(2)))
    return seen


def aggregate(results):
    """Merge the clients' histograms exactly (bucket-wise) and sum their throughput."""
    ordered, unordered = Histogram(), Histogram()
    ops, throughput = 0, 0.0
    for r in results:
        ordered.merge(Histogram.from_json(r["ordered"]))
        unordered.merge(Histogram.from_json(r["unordered"]))
        ops += int(r["ops"])
        throughput += float(r["throughput"])
    return {
        "clients": len(results),
        "ops": ops,
        "throughput": throughput,
        "ordered": ordered.summary_ms(),
        "unordered": unordered.summary_ms() if unordered.count else None,
    }


def _parse_ts(ts):
    return dt.datetime.fromisoformat(ts.replace("Z", "+00:00"))


def measurement_window(results):
    """(t0, t1) bracketing the clients' windows: each began at (stamp - elapsed_s), lasting measure_s."""
    starts, ends = [], []
    for r in results:
        if not r.get("_ts"):
            continue
        start = _parse_ts(r["_ts"]) - dt.timedelta(seconds=float(r["elapsed_s"]))
        starts.append(start)
        ends.append(start + dt.timedelta(seconds=float(r["measure_s"])))
    if not starts:
        return None
    fmt = lambda t: t.astimezone(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")
    return fmt(min(starts)), fmt(max(ends))


def parse_ping_avg_ms(text):
    """avg from 'rtt min/avg/max/mdev = a/b/c/d ms'."""
    m = re.search(r"=\s*([\d.]+)/([\d.]+)/([\d.]+)/([\d.]+)\s*ms", text)
    return float(m.group(2)) if m else None


# ── process helpers ───────────────────────────────────────────────────────────────────

def sh(cmd, check=False, timeout=60, **kw):
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, **kw)
    if check and p.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)} failed: {p.stderr.strip()[:300]}")
    return p


def docker(*args, **kw):
    return sh(["docker", *args], **kw)


def log(msg):
    print(f"[{dt.datetime.now().strftime('%H:%M:%S')}] {msg}", flush=True)


def git_state(path):
    if not os.path.isdir(os.path.join(path, ".git")):
        return None
    rev = sh(["git", "-C", path, "rev-parse", "--short", "HEAD"]).stdout.strip()
    dirty = len([l for l in sh(["git", "-C", path, "status", "--porcelain"]).stdout.splitlines() if l])
    return {"rev": rev, "uncommitted_files": dirty}


def provenance():
    cpu = ""
    try:
        with open("/proc/cpuinfo") as f:
            cpu = next((l.split(":", 1)[1].strip() for l in f if l.startswith("model name")), "")
    except OSError:
        pass
    return {
        "date": dt.datetime.now(dt.timezone.utc).isoformat(),
        "host_cpus": os.cpu_count(),
        "cpu_model": cpu,
        "kernel": os.uname().release,
        "docker": docker("version", "--format", "{{.Server.Version}}").stdout.strip(),
        "repos": {name: git_state(os.path.join(ATLAS_ROOT, name))
                  for name in ("Atlas", "febft", "testing-grounds")},
    }


# ── cluster lifecycle ─────────────────────────────────────────────────────────────────

SUITE_MARKER = ".suite-owned"


def reset_logs(keep_as=None):
    """Empty generated/logs for the next run without deleting what we did not create.

    A directory without our marker is moved aside; a marked one is emptied (via a container,
    since nodes write as root) or, to keep a failed run's logs, renamed."""
    logs = os.path.join(GENERATED, "logs")

    if os.path.isdir(logs):
        owned = os.path.exists(os.path.join(logs, SUITE_MARKER))
        if keep_as and owned:
            dest = os.path.join(GENERATED, keep_as)
            if os.path.exists(dest):
                if os.path.exists(os.path.join(dest, SUITE_MARKER)):
                    docker("run", "--rm", "-v", f"{dest}:/l", "busybox", "sh", "-c",
                           "rm -rf /l/* /l/.[!.]* 2>/dev/null; true", timeout=300)
                    os.rmdir(dest)
                else:
                    dest += f".{dt.datetime.now().strftime('%Y%m%d-%H%M%S')}"
            os.rename(logs, dest)
        elif not owned and os.listdir(logs):
            aside = f"logs.before-suite.{dt.datetime.now().strftime('%Y%m%d-%H%M%S')}"
            os.rename(logs, os.path.join(GENERATED, aside))
            log(f"generated/logs held logs this suite did not create; kept intact as {aside}")
        elif owned:
            docker("run", "--rm", "-v", f"{logs}:/l", "busybox", "sh", "-c",
                   "rm -rf /l/* /l/.[!.]* 2>/dev/null; true", timeout=300)

    os.makedirs(logs, exist_ok=True)
    open(os.path.join(logs, SUITE_MARKER), "w").close()


def containers_running():
    names = docker("ps", "--format", "{{.Names}}").stdout.split()
    return [n for n in names if re.fullmatch(r"(replica|client)-\d+", n)]


def console(container):
    p = docker("logs", container, timeout=60)
    return (p.stdout or "") + (p.stderr or "")


def start_cluster(make_args, make_log):
    cmd = ["make", "-C", TG, "crud_perf", "local", *[f"{k}={v}" for k, v in make_args.items()]]
    return subprocess.Popen(cmd, stdout=make_log, stderr=subprocess.STDOUT,
                            start_new_session=True)


def stop_cluster(proc):
    sh(["make", "-C", TG, "crud_perf", "stop-local"], timeout=180)
    try:
        proc.wait(timeout=90)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGTERM)
        try:
            proc.wait(timeout=30)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
    for name in containers_running():
        docker("rm", "-f", name)


def noteworthy_log_lines(text, n=12):
    """The last few lines of a container's output that are not connection-retry chatter."""
    noise = ("Connection broken", "Connection closed", "Reading ", "Parsing IP")
    lines = [l for l in text.splitlines() if l.strip() and not any(k in l for k in noise)]
    return "\n".join(l[:300] for l in lines[-n:])


def check_containers_alive(containers):
    """A container that restarted crashed. `restart: on-failure` would otherwise hide that
    until the run timed out, many minutes later."""
    for c in containers:
        p = docker("inspect", "-f", "{{.RestartCount}} {{.State.ExitCode}}", c)
        parts = p.stdout.split()
        if len(parts) == 2 and parts[0].isdigit() and int(parts[0]) > 0:
            raise RunFailed(
                f"{c} crashed and restarted (exit code {parts[1]}). Last output:\n"
                + noteworthy_log_lines(console(c)))


def container_running(name):
    p = docker("inspect", "-f", "{{.State.Running}}", name)
    return p.stdout.strip() == "true"


def wait_for_results(proc, expected, n_client_machines, n_replicas, run_budget_s,
                     stall_grace_s=90, build_budget_s=1800, warm_s=0, boot_grace_s=180,
                     phase_grace_s=45):
    """Wait for every client's RESULT, failing early on missed phase boundaries or silent stragglers."""
    containers = [f"client-{i}" for i in range(n_client_machines)]
    watched = containers + [f"replica-{i}" for i in range(n_replicas)]
    first_result_at = None
    results = []
    t_all_warm = t_all_meas = None

    # The run budget starts once the client container is up; image builds are timed separately.
    started = time.time()
    up_at = None
    deadline = started + build_budget_s
    while time.time() < deadline:
        if up_at is None and container_running(containers[0]):
            up_at = time.time()
            deadline = up_at + run_budget_s
        if proc.poll() is not None:
            raise RunFailed(f"make exited early with status {proc.returncode} "
                            "(build failure? see the .make.log)")
        check_containers_alive(watched)
        results, texts = [], []
        for c in containers:
            texts.append(console(c))
            results += parse_results(texts[-1])
        if len(results) >= expected:
            return results
        if up_at is None:
            time.sleep(3)
            continue

        # Fail on a missed phase boundary rather than waiting out the overall budget.
        phases = {"warmup_start": set(), "measure_start": set()}
        for t in texts:
            for k, v in parse_phases(t).items():
                phases[k] |= v
        now = time.time()
        if len(phases["warmup_start"]) >= expected and t_all_warm is None:
            t_all_warm = now
        if len(phases["measure_start"]) >= expected and t_all_meas is None:
            t_all_meas = now

        if t_all_warm is None and now - up_at > boot_grace_s:
            raise RunTimeout(
                f"{len(phases['warmup_start'])} of {expected} clients had started warming up "
                f"{boot_grace_s}s after the cluster came up: bootstrap is stuck")
        if t_all_warm and t_all_meas is None and now - t_all_warm > warm_s + phase_grace_s:
            raise RunTimeout(
                f"warm-up should have ended {phase_grace_s}s ago but only "
                f"{len(phases['measure_start'])} of {expected} clients reached measurement: "
                "requests are not being answered")
        if results and first_result_at is None:
            first_result_at = time.time()
        if first_result_at and time.time() - first_result_at > stall_grace_s:
            done = sorted(r["client"] for r in results)
            raise RunTimeout(
                f"{len(results)} of {expected} clients finished (ids {done}); the rest were still "
                f"silent {stall_grace_s}s after the first, so they are hung")
        time.sleep(3)
    raise RunTimeout(f"{len(results)} of {expected} clients reported before the deadline")


def measure_rtts(n_replicas):
    out = {}
    for j in range(1, n_replicas):
        p = docker("exec", "replica-0", "ping", "-c", "8", "-i", "0.2", "-q", f"replica-{j}",
                   timeout=60)
        out[f"replica-0->replica-{j}"] = parse_ping_avg_ms(p.stdout)
    return out


def restart_counts(n_replicas):
    counts = {}
    for i in range(n_replicas):
        p = docker("inspect", "-f", "{{.RestartCount}}", f"replica-{i}")
        counts[f"replica-{i}"] = int(p.stdout.strip()) if p.stdout.strip().isdigit() else None
    return counts


def count_startup_drops(n_replicas):
    """Application messages a replica rejected as unauthenticated. At start-up a client can begin
    sending before a replica has finished authenticating its connection, and those requests
    vanish (see the reconfig-join note). Three healthy replicas still make a quorum, so a run
    survives it, but it is a defect in the run's start and worth surfacing."""
    out = {}
    for i in range(n_replicas):
        text = console(f"replica-{i}")
        out[f"replica-{i}"] = sum(1 for l in text.splitlines() if "UnAuthenticatedMessage" in l)
    return out


def container_images(n_replicas, n_machines):
    """Image each node actually ran. The variant is compiled into the image, and the image tag
    encodes it (crud-perf-crud-single), so this is independent evidence of which executor was
    under test -- the run id merely *asserts* it."""
    names = [f"replica-{i}" for i in range(n_replicas)] + [f"client-{i}" for i in range(n_machines)]
    return {n: docker("inspect", "-f", "{{.Config.Image}}", n).stdout.strip() for n in names}


def influx_client(url):
    cfg_path = os.path.join(BENCH, "config-base", "common", "influx_db.toml")
    with open(cfg_path, "rb") as f:
        cfg = tomllib.load(f)
    return Influx(url, cfg.get("db_name", "atlas"), cfg.get("user"), cfg.get("password"))


# ── one run ───────────────────────────────────────────────────────────────────────────

def execute(run, out_dir, args):
    plan = run.plan
    fixed = plan["fixed"]
    n_replicas = int(fixed["N_REPLICAS"])
    n_machines = int(fixed.get("N_CLIENT_MACHINES", 1))
    expected = int(fixed["N_CLIENTS"]) * n_machines

    warm, meas = int(fixed["WARMUP_SECS"]), int(fixed["MEASURE_SECS"])
    # Boot + bootstrap is ~30-60 s on a LAN and grows with the RTT.
    run_budget = (int(plan.get("startup_timeout_s", 240)) + warm + meas
                  + int(plan.get("drain_timeout_s", 90)))
    build_budget = int(plan.get("build_timeout_s", 1800))
    stall_grace = int(plan.get("stall_grace_s", 90))
    boot_grace = int(plan.get("boot_grace_s", 180))
    phase_grace = int(plan.get("phase_grace_s", 45))

    record = {
        "run_id": run.run_id, "plan": plan["name"], "idx": run.idx, "rep": run.rep,
        "variant": run.variant, "factors": run.factors, "make_args": run.make_args(),
        "started": dt.datetime.now(dt.timezone.utc).isoformat(), "status": "failed",
    }

    raw = os.path.join(out_dir, "raw")
    os.makedirs(raw, exist_ok=True)
    make_log = open(os.path.join(raw, f"{run.run_id}.make.log"), "w")

    reset_logs()
    proc = start_cluster(run.make_args(), make_log)
    keep_logs = False
    try:
        results = wait_for_results(proc, expected, n_machines, n_replicas, run_budget,
                                   stall_grace, build_budget, warm, boot_grace, phase_grace)
        record["clients"] = [{k: v for k, v in r.items() if k not in ("ordered", "unordered")}
                             for r in results]
        with open(os.path.join(raw, f"{run.run_id}.results.jsonl"), "w") as f:
            for r in results:
                f.write(json.dumps(r) + "\n")

        record["agg"] = aggregate(results)

        if str(fixed.get("WAN_ENABLED", "0")) == "1":
            record["rtt_measured_ms"] = measure_rtts(n_replicas)
        record["replica_restarts"] = restart_counts(n_replicas)
        record["images"] = container_images(n_replicas, n_machines)
        record["startup_drops"] = count_startup_drops(n_replicas)

        window = measurement_window(results)
        record["window"] = window
        if window:
            time.sleep(3)  # the metrics thread writes once a second; let the last cycle land
            try:
                record["replica_metrics"] = influx_client(args.influx_url).replica_stats(
                    run.run_id, *window)
            except Exception as e:  # noqa: BLE001 -- a thin metrics series qualifies a run, it does not fail it
                record["replica_metrics_error"] = str(e)[:300]

        record["status"] = "ok"
    except RunTimeout as e:
        record.update(status="timeout", error=str(e))
        try:
            record["startup_drops"] = count_startup_drops(n_replicas)
        except Exception:  # noqa: BLE001
            pass
        keep_logs = True
    except Exception as e:  # noqa: BLE001
        record.update(status="failed", error=f"{type(e).__name__}: {e}"[:500])
        keep_logs = True
    finally:
        try:
            stop_cluster(proc)
        finally:
            make_log.close()
            if keep_logs or args.keep_logs:
                try:
                    reset_logs(keep_as=f"logs.{run.run_id}")
                except OSError as e:
                    log(f"could not preserve logs: {e}")
            else:
                reset_logs()

    record["ended"] = dt.datetime.now(dt.timezone.utc).isoformat()
    return record


# ── subcommands ───────────────────────────────────────────────────────────────────────

def preflight(plan, runs, args):
    problems = []
    if shutil.which("docker") is None:
        problems.append("docker is not on PATH")
        return problems

    running = containers_running()
    if running:
        problems.append(f"containers from another run are up: {', '.join(running)} "
                        "(make crud_perf stop-local)")

    for name in sorted({wan.profile_name(r.factors["rtt_ms"]) for r in runs if "rtt_ms" in r.factors}):
        if not os.path.isfile(os.path.join(BENCH, "wan-profiles", name + ".yml")):
            problems.append(f"missing WAN profile {name}.yml (run suite/gen_wan_profiles.py)")

    fixed = plan["fixed"]
    if "WAN_PROFILE" in fixed and not os.path.isfile(
            os.path.join(BENCH, "wan-profiles", f"{fixed['WAN_PROFILE']}.yml")):
        problems.append(f"missing WAN profile {fixed['WAN_PROFILE']}.yml")

    if str(fixed.get("WAN_ENABLED", "0")) == "1":
        want = str(fixed.get("WAN_SUBNET", "10.90.0.0/24"))
        have = docker("network", "inspect", "atlas_network", "-f",
                      "{{range .IPAM.Config}}{{.Subnet}}{{end}}").stdout.strip()
        if have and have != want:
            problems.append(
                f"atlas_network has subnet {have} but WAN mode needs {want}. The metrics stack "
                "holds the network open; run `make stop-metrics` first, then re-run.")

    # The PKI is never extended; a missing key panics the node (reported only as exit 139).
    ca_root = os.path.join(GENERATED, "ca-root")
    if os.path.isdir(ca_root):
        ids = list(range(int(fixed["N_REPLICAS"]))) + [
            1000 + i for i in range(int(fixed["N_CLIENTS"]))]
        missing = [i for i in ids
                   if not os.path.exists(os.path.join(ca_root, f"ca-root-{i}-private_pem"))]
        if missing:
            problems.append(
                f"generated/ca-root has no key for node(s) {missing}. Regenerate with: "
                f"make crud_perf regen-ca-root N_REPLICAS={fixed['N_REPLICAS']} "
                f"N_CLIENTS={fixed['N_CLIENTS']}")

    need = (int(fixed.get("PIN_CPU_OFFSET", 0))
            + int(fixed["N_REPLICAS"]) * int(fixed.get("REPLICA_CORES", 0))
            + int(fixed.get("N_CLIENT_MACHINES", 1)) * int(fixed.get("CLIENT_CORES", 0)))
    if str(fixed.get("PIN_CPUS", "0")) == "1" and need > (os.cpu_count() or 0):
        problems.append(f"CPU pinning needs {need} cores, host has {os.cpu_count()}")

    free_gb = shutil.disk_usage(GENERATED if os.path.isdir(GENERATED) else TG).free / 2**30
    if free_gb < 20:
        problems.append(f"only {free_gb:.0f} GB free next to generated/logs")
    return problems


def cmd_plan(args):
    plan = planlib.load(args.plan)
    runs = planlib.expand(plan, quick=args.quick)
    est = planlib.estimate_seconds(plan, runs)

    print(f"plan        {plan['name']}  [{planlib.fingerprint(plan)}]")
    print(f"            {(plan.get('description') or '').strip().splitlines()[0] if plan.get('description') else ''}")
    print(f"variants    {', '.join(plan['variants'])}")
    for name, levels in (plan.get("factors") or {}).items():
        print(f"factor      {name}: {levels}")
    print(f"repetitions {1 if args.quick else plan.get('repetitions', 1)}"
          f"{'  (--quick)' if args.quick else ''}")
    print(f"runs        {len(runs)}")
    print(f"estimate    {est / 3600:.1f} h  "
          f"({int(plan['fixed']['WARMUP_SECS'])}s warm-up + {int(plan['fixed']['MEASURE_SECS'])}s "
          f"measure + ~75s boot/teardown per run)")
    print("\nfirst runs (execution order):")
    for r in runs[:12]:
        print(f"  {r.idx:4d}  rep {r.rep}  {r.variant:14s} {r.factors}")
    if len(runs) > 12:
        print(f"  ... {len(runs) - 12} more")

    problems = preflight(plan, runs, args)
    if problems:
        print("\npreflight problems:")
        for p in problems:
            print(f"  - {p}")
        return 1
    print("\npreflight ok")
    return 0


def _raise_interrupt(signum, _frame):
    raise KeyboardInterrupt(f"signal {signum}")


def cmd_run(args):
    # `nohup cmd &` ignores SIGINT and SIGTERM kills without cleanup; unwind through `finally`.
    signal.signal(signal.SIGINT, _raise_interrupt)
    signal.signal(signal.SIGTERM, _raise_interrupt)

    plan = planlib.load(args.plan)
    runs = planlib.expand(plan, quick=args.quick)
    if args.variants:
        wanted = set(args.variants.split(","))
        runs = [r for r in runs if r.variant in wanted]

    out_dir = os.path.abspath(args.out or os.path.join(SUITE_DIR, "results", plan["name"]))
    os.makedirs(out_dir, exist_ok=True)
    runs_path = os.path.join(out_dir, "runs.jsonl")
    meta_path = os.path.join(out_dir, "meta.json")

    fp = planlib.fingerprint(plan)
    done = set()
    if os.path.exists(meta_path):
        with open(meta_path) as f:
            meta = json.load(f)
        if meta.get("fingerprint") != fp or meta.get("quick") != bool(args.quick):
            sys.exit(f"{out_dir} holds results of a different plan/shape "
                     f"({meta.get('fingerprint')} vs {fp}); use another --out")
        if os.path.exists(runs_path) and not args.resume:
            sys.exit(f"{runs_path} already has results; pass --resume to continue the sweep")
        if os.path.exists(runs_path):
            with open(runs_path) as f:
                done = {json.loads(l)["run_id"] for l in f if l.strip()
                        if json.loads(l).get("status") == "ok"}
    else:
        with open(meta_path, "w") as f:
            json.dump({"fingerprint": fp, "quick": bool(args.quick), "plan_file": plan["_path"],
                       "plan": {k: v for k, v in plan.items() if not k.startswith("_")},
                       "provenance": provenance()}, f, indent=2, default=str)

    todo = [r for r in runs if r.run_id not in done]
    if args.limit:
        todo = todo[: args.limit]

    problems = preflight(plan, runs, args)
    if problems and not args.dry_run:
        print("preflight problems:\n  - " + "\n  - ".join(problems))
        return 1

    log(f"{len(todo)} runs to execute ({len(done)} already done) -> {out_dir}")
    if args.dry_run:
        for r in todo:
            print(f"{r.run_id}: make crud_perf local " + " ".join(f"{k}={v}" for k, v in r.make_args().items()))
        return 0

    consecutive_failures = 0
    for n, run in enumerate(todo, 1):
        log(f"[{n}/{len(todo)}] {run.run_id}  rep={run.rep}  {run.factors}")

        # Retries run later than their block; `attempt` is recorded, analysis takes the last.
        record = None
        for attempt in range(args.retries + 1):
            try:
                record = execute(run, out_dir, args)
            except KeyboardInterrupt:
                log("interrupted; cluster torn down. Re-run with --resume to continue.")
                return 130
            record["attempt"] = attempt
            with open(runs_path, "a") as f:
                f.write(json.dumps(record) + "\n")
                f.flush()
                os.fsync(f.fileno())

            if record["status"] == "ok":
                break
            log(f"    {record['status'].upper()} (attempt {attempt + 1}/{args.retries + 1}): "
                f"{record.get('error')}")

        if record["status"] == "ok":
            consecutive_failures = 0
            a = record["agg"]["ordered"]
            drops = sum((record.get("startup_drops") or {}).values())
            log(f"    ok   {record['agg']['throughput']:.0f} ops/s   "
                f"p50 {a.get('p50_ms', float('nan')):.2f} ms   p99 {a.get('p99_ms', float('nan')):.2f} ms"
                + (f"   [start-up dropped {drops} messages]" if drops else ""))
        else:
            consecutive_failures += 1
            if consecutive_failures >= args.max_consecutive_failures:
                log(f"{consecutive_failures} runs in a row failed every attempt; stopping "
                    "(something is wrong with the environment, not the runs).")
                return 2
    log("sweep complete")
    return 0


def cmd_status(args):
    path = os.path.join(args.dir, "runs.jsonl")
    if not os.path.exists(path):
        print("no runs yet")
        return 0
    counts = {}
    with open(path) as f:
        for line in f:
            if line.strip():
                r = json.loads(line)
                counts.setdefault(r["variant"], {}).setdefault(r["status"], 0)
                counts[r["variant"]][r["status"]] += 1
    for variant, c in sorted(counts.items()):
        print(f"{variant:14s} " + "  ".join(f"{k}={v}" for k, v in sorted(c.items())))
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    for name in ("plan", "run"):
        p = sub.add_parser(name)
        p.add_argument("plan")
        p.add_argument("--quick", action="store_true",
                       help="first and last level of each factor, one repetition (smoke test)")
    sub.choices["run"].add_argument("--out")
    sub.choices["run"].add_argument("--resume", action="store_true")
    sub.choices["run"].add_argument("--limit", type=int)
    sub.choices["run"].add_argument("--variants", help="comma-separated subset to run")
    sub.choices["run"].add_argument("--dry-run", action="store_true")
    sub.choices["run"].add_argument("--keep-logs", action="store_true",
                                    help="keep every run's node logs, not just failures'")
    sub.choices["run"].add_argument("--influx-url", default="http://localhost:8086")
    sub.choices["run"].add_argument("--max-consecutive-failures", type=int, default=3)
    sub.choices["run"].add_argument("--retries", type=int, default=2,
                                    help="extra attempts for a run that fails (default 2)")
    p = sub.add_parser("status")
    p.add_argument("dir")

    args = ap.parse_args()
    sys.exit({"plan": cmd_plan, "run": cmd_run, "status": cmd_status}[args.cmd](args))


if __name__ == "__main__":
    main()
