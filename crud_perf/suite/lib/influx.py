"""Minimal InfluxDB 1.x client for replica-side run facts (hit rate, backtracks, batch size).
Client latency does not come from here."""

import base64
import json
import urllib.parse
import urllib.request

# metric name -> how to reduce it over the measurement window
REPLICA_METRICS = {
    "SPECULATION_HIT_RATE": "mean",     # permille: 1000 = every commit served from a speculative reply
    "DS_BACKTRACK_COUNT": "sum",
    "CACHE_BACKTRACK_COUNT": "sum",     # crud_single and (since 2026-10-31) crud_scalable
    "SPECULATION_FALLBACK_COUNT": "sum",
    "BATCH_SIZE": "mean",
    "CONSENSUS_WAIT_TIME": "mean",      # ns
    "CONFIRM_TO_REPLY_TIME": "mean",    # ns
    "OS_CPU_USER": "mean",
}


class Influx:
    def __init__(self, url, db, user=None, password=None, timeout=10):
        self.url = url.rstrip("/")
        self.db = db
        self.timeout = timeout
        self.auth = None
        if user:
            token = base64.b64encode(f"{user}:{password or ''}".encode()).decode()
            self.auth = f"Basic {token}"

    def query(self, q):
        params = urllib.parse.urlencode({"db": self.db, "q": q})
        req = urllib.request.Request(f"{self.url}/query?{params}")
        if self.auth:
            req.add_header("Authorization", self.auth)
        with urllib.request.urlopen(req, timeout=self.timeout) as resp:
            body = json.load(resp)
        for result in body.get("results", []):
            if "error" in result:
                raise RuntimeError(result["error"])
        return body

    def replica_stats(self, tag, t0, t1):
        """{metric: {"value": reduced across replicas, "per_host": {host: value}}} for the
        points tagged `tag` in [t0, t1] (ISO-8601). Client nodes (id >= 1000) are excluded."""
        out = {}
        for metric, how in REPLICA_METRICS.items():
            q = (f'SELECT {how}("value") FROM "{metric}" '
                 f"WHERE \"extra\" = '{tag}' AND time >= '{t0}' AND time <= '{t1}' "
                 f'GROUP BY "host"')
            per_host = {}
            for result in self.query(q).get("results", []):
                for series in result.get("series", []) or []:
                    host = series["tags"]["host"]
                    node = int(host[len("NodeId("):-1]) if host.startswith("NodeId(") else None
                    if node is not None and node >= 1000:
                        continue
                    val = series["values"][0][1]
                    if val is not None:
                        per_host[host] = val
            if per_host:
                vals = list(per_host.values())
                out[metric] = {
                    "value": sum(vals) if how == "sum" else sum(vals) / len(vals),
                    "per_host": per_host,
                }
        return out
