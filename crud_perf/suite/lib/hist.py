"""Decode and merge the client's latency histograms.

Mirrors `histogram.rs` bucket for bucket (pinned by shared golden vectors). Histograms merge by
adding buckets; averaging per-client percentiles would be wrong."""

import math

SUB_BITS = 6
SUB_COUNT = 1 << SUB_BITS


def bucket_index(value):
    if value < SUB_COUNT:
        return value
    msb = value.bit_length() - 1
    shift = msb - SUB_BITS
    return ((shift + 1) << SUB_BITS) + ((value >> shift) & (SUB_COUNT - 1))


def bucket_lower(index):
    if index < SUB_COUNT:
        return index
    octave = index >> SUB_BITS
    sub = index & (SUB_COUNT - 1)
    return (SUB_COUNT + sub) << (octave - 1)


def bucket_value(index):
    """A representative value for a bucket: its midpoint, so the error is centred on zero
    rather than always biased low."""
    lo = bucket_lower(index)
    hi = bucket_lower(index + 1)
    return lo if hi - lo <= 1 else (lo + hi) / 2


class Histogram:
    def __init__(self):
        self.buckets = {}
        self.count = 0
        self.sum_ns = 0
        self.max_ns = 0

    @classmethod
    def from_json(cls, d):
        h = cls()
        h.count = int(d["count"])
        h.sum_ns = int(d["sum_ns"])
        h.max_ns = int(d["max_ns"])
        h.buckets = {int(k): int(v) for k, v in d["buckets"].items()}
        total = sum(h.buckets.values())
        if total != h.count:
            raise ValueError(f"histogram is inconsistent: count={h.count} but buckets sum to {total}")
        return h

    def merge(self, other):
        for k, v in other.buckets.items():
            self.buckets[k] = self.buckets.get(k, 0) + v
        self.count += other.count
        self.sum_ns += other.sum_ns
        self.max_ns = max(self.max_ns, other.max_ns)
        return self

    def mean_ns(self):
        return self.sum_ns / self.count if self.count else None

    def percentile_ns(self, p):
        """Value below which `p` percent of samples fall (nearest-rank)."""
        if not self.count:
            return None
        if p >= 100:
            return float(self.max_ns)
        rank = max(1, math.ceil(p / 100.0 * self.count))
        seen = 0
        for index in sorted(self.buckets):
            seen += self.buckets[index]
            if seen >= rank:
                return float(bucket_value(index))
        return float(self.max_ns)

    def summary_ms(self):
        """The numbers a result table wants, in milliseconds."""
        if not self.count:
            return {"count": 0}
        ms = lambda ns: None if ns is None else ns / 1e6
        return {
            "count": self.count,
            "mean_ms": ms(self.mean_ns()),
            "p50_ms": ms(self.percentile_ns(50)),
            "p90_ms": ms(self.percentile_ns(90)),
            "p99_ms": ms(self.percentile_ns(99)),
            "p999_ms": ms(self.percentile_ns(99.9)),
            "max_ms": ms(float(self.max_ns)),
        }
