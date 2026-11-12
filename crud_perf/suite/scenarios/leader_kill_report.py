"""Per-second decided requests and view installs at replica 1 after the leader stop, from the node logs."""
import glob, json, re, datetime as dt, os, collections

L = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "bench", "generated", "logs")

f = lambda s: dt.datetime.fromisoformat(s.replace("Z", "+00:00"))
rows = []
for fn in glob.glob(f"{L}/log_[0-3]/atlas_debug_*.log.*"):
    who = os.path.basename(os.path.dirname(fn))
    for l in open(fn, errors="replace"):
        try: o = json.loads(l)
        except Exception: continue
        rows.append((f(o["timestamp"]), who, o.get("level"), o.get("fields", {}).get("message", "")))
rows.sort()
kill = max(t for t, w, l, m in rows if w == "log_0")
inst = [(round((t - kill).total_seconds(), 1), w) for t, w, l, m in rows if "Installing view" in m and w == "log_1"]
print("view installs at replica 1 (s after kill):", [i[0] for i in inst])
dec = collections.Counter()
for t, w, l, m in rows:
    x = re.search(r"Finalizing consensus instance SeqNo\((\d+)\) with (\d+) rqs", m)
    if x and w == "log_1" and t > kill: dec[int((t - kill).total_seconds())] += int(x.group(2))
print("requests decided per second after kill:", [(k, dec[k]) for k in sorted(dec)][:40])
bad = collections.Counter(re.sub(r"\d+", "N", m)[:80] for t, w, l, m in rows if t > kill and l == "ERROR" and "Stack" not in m)
print("errors after kill:", dict(bad))
