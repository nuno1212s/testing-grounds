#!/usr/bin/env python3
"""gen-wan.py — compile a WAN profile into per-node tc/netem scripts.

Reads a declarative profile from bench/wan-profiles/<WAN_PROFILE>.yml and emits,
into <GENERATED>/wan/:

  <node>.sh              literal tc commands for that node (bind-mounted at /wan)
  matrix.txt             human-readable per-direction delay/loss/rate table
  profile.resolved.yml   fully expanded effective profile, for the experiment record

All inputs come from the environment, exported by the shared bench Makefile —
the same contract the other gen-*.sh scripts use:

  GENERATED, GLOBAL_BENCH_DIR, WAN_PROFILE, N_REPLICAS, N_CLIENT_MACHINES
  WAN_SUBNET, WAN_IFACE, WAN_SHAPE_CLIENTS, WAN_TIMEOUT_SCALE, LOCAL_INFLUXDB

Semantics — read this before writing a profile
----------------------------------------------
Shaping is applied on EGRESS at both ends of a link, so a profile value that
describes the round-trip is split across the two directions. The splits are the
mathematically exact ones, not approximations:

  delay   owd      = rtt_ms / 2                 (delays add)
  jitter  sigma    = jitter_ms / sqrt(2)        (independent variances add)
  loss    p        = 1 - sqrt(1 - loss_pct/100) (round-trip success = (1-p)^2)

A direction may instead give `owd_ms` / `owd_jitter_ms` / `owd_loss_pct`, which
are taken verbatim with no splitting — that is how asymmetric links are written.
"""

import fnmatch
import ipaddress
import math
import os
import re
import sys

import yaml

MTU = 1500


def fmt(x):
    """Trim float noise: 39.0 -> '39', 2.8284 -> '2.828'."""
    return f"{float(x):g}"


DEFAULT_CLASS = "999"          # htb default class (tc reads classid minors as hex)
PEER_CLASS_BASE = 10           # first per-peer class minor


# ── env plumbing ──────────────────────────────────────────────────────────────────

def env(name, default=None, required=False):
    val = os.environ.get(name, "")
    if val == "":
        if required:
            die(f"{name} not set (this script is invoked by the shared bench Makefile)")
        return default
    return val


def die(msg):
    print(f"ERROR: {msg}", file=sys.stderr)
    sys.exit(1)


def warn(msg):
    print(f"WARNING: {msg}", file=sys.stderr)


# ── rate parsing ──────────────────────────────────────────────────────────────────

_RATE_UNITS = {
    "bit": 1, "kbit": 1_000, "mbit": 1_000_000, "gbit": 1_000_000_000,
    "tbit": 1_000_000_000_000,
    "bps": 8, "kbps": 8_000, "mbps": 8_000_000, "gbps": 8_000_000_000,
}


def parse_rate(rate):
    """'1gbit' -> 1_000_000_000 (bits/sec). Accepts the units tc itself accepts."""
    s = str(rate).strip().lower().replace(" ", "")
    m = re.fullmatch(r"([0-9]*\.?[0-9]+)([a-z]+)", s)
    if not m:
        die(f"cannot parse rate {rate!r} (expected e.g. '1gbit', '100mbit')")
    value, unit = float(m.group(1)), m.group(2)
    if unit not in _RATE_UNITS:
        die(f"unknown rate unit {unit!r} in {rate!r}; known: {', '.join(sorted(_RATE_UNITS))}")
    return value * _RATE_UNITS[unit]


# ── node inventory and addressing ─────────────────────────────────────────────────

def build_nodes(n_replicas, n_client_machines, subnet, shape_clients, local_influxdb):
    """Node name -> IP. Offsets match the plan: influx .5, replicas .10+i, clients .100+i."""
    net = ipaddress.ip_network(subnet, strict=True)

    def addr(offset, what):
        # net[offset] rather than hosts[] so the offsets are literal last-octet
        # values in the common /24 case and stay readable in wan/*.sh.
        if offset >= net.num_addresses - 1:
            die(f"WAN_SUBNET={subnet} is too small for {what} (offset {offset}); widen it")
        return str(net[offset])

    nodes = {}
    for i in range(n_replicas):
        nodes[f"replica-{i}"] = addr(10 + i, f"replica-{i}")
    for i in range(n_client_machines):
        nodes[f"client-{i}"] = addr(100 + i, f"client-{i}")

    if 10 + n_replicas > 100:
        die(f"N_REPLICAS={n_replicas} overflows the replica address block "
            f"(.10...99); widen WAN_SUBNET and adjust the offsets in gen-wan.py")

    extra = {}
    if local_influxdb == "1":
        extra["influxdb"] = addr(5, "influxdb")

    # Which nodes actually receive a shaping spec.
    shaped = [n for n in nodes if shape_clients or n.startswith("replica-")]
    return nodes, extra, shaped


# ── profile resolution ────────────────────────────────────────────────────────────

def region_of(node, placement):
    """First matching glob wins (dict order is insertion order in py3.7+)."""
    for pattern, region in placement.items():
        if fnmatch.fnmatchcase(node, pattern):
            return region
    return None


def merge(base, *overlays):
    out = dict(base)
    for o in overlays:
        if o:
            out.update({k: v for k, v in o.items() if v is not None})
    return out


def link_params(profile, src, dst, regions):
    """Resolve the effective profile-level parameters for the directed pair src->dst."""
    params = dict(profile["defaults"])

    r_src, r_dst = regions[src], regions[dst]
    if r_src == r_dst:
        params = merge(params, profile.get("intra_region"))
    else:
        for link in profile.get("links") or []:
            a, b = link.get("a"), link.get("b")
            if {a, b} == {r_src, r_dst}:
                params = merge(params, {k: v for k, v in link.items() if k not in ("a", "b")})
                break

    # Overrides, least specific first so the most specific wins.
    for ov in profile.get("overrides") or []:
        between = ov.get("between")
        if between and len(between) == 2:
            x, y = between
            if (fnmatch.fnmatchcase(src, x) and fnmatch.fnmatchcase(dst, y)) or \
               (fnmatch.fnmatchcase(src, y) and fnmatch.fnmatchcase(dst, x)):
                params = merge(params, {k: v for k, v in ov.items() if k != "between"})
    for ov in profile.get("overrides") or []:
        frm, to = ov.get("from"), ov.get("to")
        if frm and to and fnmatch.fnmatchcase(src, frm) and fnmatch.fnmatchcase(dst, to):
            params = merge(params, {k: v for k, v in ov.items() if k not in ("from", "to")})

    return params


def to_one_way(params):
    """Split round-trip profile values into the per-direction values tc needs."""
    if "owd_ms" in params:
        owd = float(params["owd_ms"])
        jitter = float(params.get("owd_jitter_ms", params.get("jitter_ms", 0)) or 0)
        if "owd_jitter_ms" not in params and params.get("jitter_ms"):
            jitter = float(params["jitter_ms"]) / math.sqrt(2)
    else:
        owd = float(params.get("rtt_ms", 0) or 0) / 2.0
        jitter = float(params.get("jitter_ms", 0) or 0) / math.sqrt(2)

    if "owd_loss_pct" in params:
        loss = float(params["owd_loss_pct"])
    else:
        rt_loss = float(params.get("loss_pct", 0) or 0) / 100.0
        if rt_loss <= 0:
            loss = 0.0
        elif rt_loss >= 1:
            loss = 100.0
        else:
            loss = (1.0 - math.sqrt(1.0 - rt_loss)) * 100.0

    return {
        "owd_ms": round(owd, 4),
        "jitter_ms": round(jitter, 3),
        "loss_pct": round(loss, 4),
        "rate": params.get("rate", "10gbit"),
        "distribution": params.get("distribution", "normal"),
        "netem_limit": params.get("netem_limit", "auto"),
        "duplicate_pct": params.get("duplicate_pct"),
        "corrupt_pct": params.get("corrupt_pct"),
        "reorder_pct": params.get("reorder_pct"),
    }


def netem_limit(spec):
    """Backlog in packets. netem's default of 1000 silently drops on a fat WAN pipe:
    at 1gbit x 40ms one-way there are ~3300 packets in flight before any burst."""
    if spec["netem_limit"] != "auto":
        return int(spec["netem_limit"])
    bps = parse_rate(spec["rate"])
    in_flight = bps * (spec["owd_ms"] / 1000.0) / (8.0 * MTU)
    return max(1000, int(math.ceil(in_flight * 4)))


# ── tc script emission ────────────────────────────────────────────────────────────

UNSHAPED_RATE = 10_000_000_000  # the default HTB ceiling; at or above it, no real cap


def is_impaired(spec):
    """True if this link needs a netem qdisc at all.

    Matters for the `none` control profile: an unconditional `netem limit 1000`
    is NOT a no-op — at 10gbit a 1000-packet backlog drops under load, which
    would make the control run quietly lossy and invalidate the comparison.
    """
    return bool(
        spec["owd_ms"] > 0 or spec["jitter_ms"] > 0 or spec["loss_pct"] > 0
        or spec.get("duplicate_pct") or spec.get("corrupt_pct") or spec.get("reorder_pct")
    )


def needs_class(spec):
    """A peer needs its own HTB class only if it is impaired or rate-limited."""
    return is_impaired(spec) or parse_rate(spec["rate"]) < UNSHAPED_RATE


def netem_args(spec):
    args = []
    if spec["owd_ms"] > 0 or spec["jitter_ms"] > 0:
        args += ["delay", f"{fmt(spec['owd_ms'])}ms"]
        if spec["jitter_ms"] > 0:
            args += [f"{fmt(spec['jitter_ms'])}ms", "distribution", str(spec["distribution"])]
    if spec["loss_pct"] > 0:
        args += ["loss", f"{fmt(spec['loss_pct'])}%"]
    if spec.get("duplicate_pct"):
        args += ["duplicate", f"{spec['duplicate_pct']}%"]
    if spec.get("corrupt_pct"):
        args += ["corrupt", f"{spec['corrupt_pct']}%"]
    if spec.get("reorder_pct"):
        args += ["reorder", f"{spec['reorder_pct']}%"]
    args += ["limit", str(netem_limit(spec))]
    return " ".join(args)


def emit_node_script(node, peers, iface, profile_name):
    """peers: list of (peer_name, peer_ip, class_minor, spec)."""
    L = [
        "#!/bin/sh",
        f"# Generated by gen-wan.py from profile '{profile_name}' — do not edit by hand.",
        f"# Egress shaping for {node} on {iface}. Regenerate with: make <project> wan-plan",
        "#",
        "# One HTB class per peer, each carrying its own netem, selected by a u32",
        "# filter on destination IP. Traffic matching no peer (host port-forwards,",
        "# DNS, InfluxDB) falls into the default class and is left unshaped.",
        "#",
        "# NOTE: tc parses classid minors as HEX. The values below are only ever",
        "# required to be unique, which they are — no arithmetic is done on them.",
        "set -e",
        "",
        f'IF="${{WAN_IFACE:-{iface}}}"',
        "",
        "# Idempotent: wipe any previous root qdisc so wan-apply can re-run this.",
        'tc qdisc del dev "$IF" root 2>/dev/null || true',
        "",
        f'tc qdisc add dev "$IF" root handle 1: htb default {DEFAULT_CLASS}',
        f'tc class add dev "$IF" parent 1: classid 1:{DEFAULT_CLASS} htb rate 10gbit',
        "",
    ]

    if not peers:
        L += ["# No shaped peers for this node.", ""]

    shaped = 0
    for peer, ip, minor, spec in peers:
        if not needs_class(spec):
            L += [f"# → {peer} ({ip}): no delay, loss or rate cap — left in the "
                  f"default class", ""]
            continue
        shaped += 1
        L += [
            f"# → {peer} ({ip}): owd {fmt(spec['owd_ms'])}ms"
            + (f" ±{fmt(spec['jitter_ms'])}ms" if spec["jitter_ms"] else "")
            + (f", loss {fmt(spec['loss_pct'])}%" if spec["loss_pct"] else "")
            + f", rate {spec['rate']}",
            f'tc class add dev "$IF" parent 1: classid 1:{minor} '
            f'htb rate {spec["rate"]} ceil {spec["rate"]}',
        ]
        if is_impaired(spec):
            L += [f'tc qdisc add dev "$IF" parent 1:{minor} handle {minor}: '
                  f'netem {netem_args(spec)}']
        L += [
            f'tc filter add dev "$IF" protocol ip parent 1:0 prio 1 '
            f'u32 match ip dst {ip}/32 flowid 1:{minor}',
            "",
        ]

    L += [f'echo "[wan] {node}: shaping applied to {shaped} peer(s) on $IF"']
    return "\n".join(L) + "\n"


# ── timeout scaling ───────────────────────────────────────────────────────────────

TIMEOUT_FILES = ["febft.toml", "log_transfer.toml", "state_transfer.toml", "view_transfer.toml"]


def scale_timeouts(generated, config_base, scale):
    """Multiply timeout_duration in the GENERATED configs only.

    The scaled value is always derived from the pristine config-base template, never
    from whatever is currently in generated/. That makes this idempotent: running
    wan-plan five times in a row gives 5x the base value once, not 5^5 times it.
    config-base itself is never written to, so a plain `make gen-configs` always
    restores the shipped values.
    """
    pattern = re.compile(r"^(\s*timeout_duration\s*=\s*)([0-9_]+)\s*$", re.MULTILINE)
    touched = []
    for name in TIMEOUT_FILES:
        base_path = os.path.join(config_base, "common", name)
        if not os.path.isfile(base_path):
            continue
        m = pattern.search(open(base_path).read())
        if not m:
            continue
        scaled = int(int(m.group(2).replace("_", "")) * scale)

        for cfg in ("config-replicas", "config-clients"):
            path = os.path.join(generated, cfg, name)
            if not os.path.isfile(path):
                continue
            new_text, n = pattern.subn(lambda mm: f"{mm.group(1)}{scaled}", open(path).read())
            if n:
                open(path, "w").write(new_text)
                touched.append(f"{cfg}/{name}")
    if touched:
        print(f"[wan] timeout_duration scaled x{fmt(scale)} from config-base in: "
              f"{', '.join(touched)}")


# ── main ──────────────────────────────────────────────────────────────────────────

def main():
    generated = env("GENERATED", required=True)
    global_bench = env("GLOBAL_BENCH_DIR", required=True)
    profile_name = env("WAN_PROFILE", required=True)
    n_replicas = int(env("N_REPLICAS", required=True))
    n_client_machines = int(env("N_CLIENT_MACHINES", required=True))
    subnet = env("WAN_SUBNET", "10.90.0.0/24")
    iface = env("WAN_IFACE", "eth0")
    shape_clients = env("WAN_SHAPE_CLIENTS", "1") == "1"
    timeout_scale = float(env("WAN_TIMEOUT_SCALE", "1"))
    local_influxdb = env("LOCAL_INFLUXDB", "0")

    profile_path = os.path.join(global_bench, "wan-profiles", f"{profile_name}.yml")
    if not os.path.isfile(profile_path):
        available = sorted(
            f[:-4] for f in os.listdir(os.path.join(global_bench, "wan-profiles"))
            if f.endswith(".yml")
        ) if os.path.isdir(os.path.join(global_bench, "wan-profiles")) else []
        die(f"profile not found: {profile_path}\n"
            f"       available: {', '.join(available) or '(none)'}")

    with open(profile_path) as f:
        profile = yaml.safe_load(f) or {}
    profile.setdefault("defaults", {})
    profile["defaults"] = merge(
        {"rtt_ms": 0, "jitter_ms": 0, "loss_pct": 0, "rate": "10gbit",
         "distribution": "normal", "netem_limit": "auto"},
        profile["defaults"],
    )

    if profile.get("schedule"):
        warn("this profile defines `schedule:` — timed events are parsed but NOT yet "
             "applied by this phase; the static profile is what will run")

    nodes, extra, shaped = build_nodes(
        n_replicas, n_client_machines, subnet, shape_clients, local_influxdb)

    # Every node in the run must be placed, or the shaping silently misses links.
    placement = profile.get("placement") or {}
    regions, unplaced = {}, []
    for node in nodes:
        r = region_of(node, placement)
        if r is None:
            unplaced.append(node)
        regions[node] = r
    if unplaced:
        die(f"profile '{profile_name}' does not place these nodes: {', '.join(unplaced)}\n"
            f"       N_REPLICAS={n_replicas} N_CLIENT_MACHINES={n_client_machines}\n"
            f"       add a `placement:` entry (globs allowed, e.g. \"replica-*\") in {profile_path}")

    known_regions = set(regions.values())
    for link in profile.get("links") or []:
        for side in ("a", "b"):
            if link.get(side) not in known_regions:
                warn(f"link references region {link.get(side)!r} which no node is placed in")

    # ── resolve every directed pair ───────────────────────────────────────────────
    resolved = {}
    for src in shaped:
        peers = []
        minor = PEER_CLASS_BASE
        for dst in nodes:
            if dst == src:
                continue
            if not shape_clients and dst.startswith("client-"):
                continue
            spec = to_one_way(link_params(profile, src, dst, regions))
            peers.append((dst, nodes[dst], str(minor), spec))
            minor += 1
        resolved[src] = peers

    if any(p[3]["jitter_ms"] > 0 for ps in resolved.values() for p in ps):
        warn("this profile uses jitter — netem may reorder packets. "
             "Set jitter_ms: 0 for strictly reproducible, in-order runs.")

    # ── write output ──────────────────────────────────────────────────────────────
    outdir = os.path.join(generated, "wan")
    os.makedirs(outdir, exist_ok=True)
    for stale in os.listdir(outdir):
        if stale.endswith(".sh"):
            os.remove(os.path.join(outdir, stale))

    for node, peers in resolved.items():
        path = os.path.join(outdir, f"{node}.sh")
        with open(path, "w") as f:
            f.write(emit_node_script(node, peers, iface, profile_name))
        os.chmod(path, 0o755)

    matrix = render_matrix(profile_name, nodes, extra, regions, resolved, shape_clients)
    with open(os.path.join(outdir, "matrix.txt"), "w") as f:
        f.write(matrix)

    with open(os.path.join(outdir, "profile.resolved.yml"), "w") as f:
        yaml.safe_dump({
            "profile": profile_name,
            "generated_from": profile_path,
            "subnet": subnet,
            "iface": iface,
            "shape_clients": shape_clients,
            "timeout_scale": timeout_scale,
            "addresses": {**nodes, **extra},
            "regions": regions,
            "links": {
                src: {p[0]: {"ip": p[1], "class": f"1:{p[2]}",
                             **{k: v for k, v in p[3].items() if v is not None}}
                      for p in peers}
                for src, peers in resolved.items()
            },
            "schedule": profile.get("schedule") or [],
        }, f, sort_keys=False, default_flow_style=False)

    if timeout_scale != 1:
        scale_timeouts(generated, os.path.join(global_bench, "config-base"), timeout_scale)

    print(f"Written {outdir}/ ({len(resolved)} node scripts, profile '{profile_name}')")


def render_matrix(profile_name, nodes, extra, regions, resolved, shape_clients):
    names = list(nodes)

    def cell_for(src, dst):
        if src == dst:
            return "."
        peers = {p[0]: p[3] for p in resolved.get(src, [])}
        if dst not in peers:
            return "-"
        s = peers[dst]
        out = fmt(s["owd_ms"])
        if s["jitter_ms"]:
            out += f"±{fmt(s['jitter_ms'])}"
        if s["loss_pct"]:
            out += f"/{fmt(s['loss_pct'])}%"
        return out

    cells = {(a, b): cell_for(a, b) for a in names for b in names}
    # Column width follows the widest thing that has to fit in it.
    w = max([len(n) for n in names] + [len(c) for c in cells.values()]) + 2
    label_w = max(len(n) for n in names) + 2

    L = [
        f"WAN profile: {profile_name}",
        "",
        "Per-direction (one-way) delay in ms, as delay±jitter/loss.",
        "Row = source, column = destination. Shaping is egress-only at both ends,",
        "so the round-trip between i and j is cell[i][j] + cell[j][i].",
        "'-' = not shaped by this node (falls into the unshaped default class).",
        "",
        "".ljust(label_w) + "".join(n.ljust(w) for n in names),
    ]
    for src in names:
        L.append(src.ljust(label_w) + "".join(cells[(src, dst)].ljust(w) for dst in names))

    L += ["", "Node placement and addressing:", ""]
    nw = max(len(n) for n in names)
    for n in names:
        L.append(f"  {n.ljust(nw)}  {nodes[n].ljust(16)}  region={regions[n]}")
    for n, ip in extra.items():
        L.append(f"  {n.ljust(nw)}  {ip.ljust(16)}  (unshaped)")

    rates = {p[3]["rate"] for ps in resolved.values() for p in ps}
    L += ["", f"Bandwidth ceilings in use: {', '.join(sorted(rates)) or '(none)'}"]
    if not shape_clients:
        L.append("WAN_SHAPE_CLIENTS=0 — client traffic is unshaped in both directions.")
    return "\n".join(L) + "\n"


if __name__ == "__main__":
    main()
