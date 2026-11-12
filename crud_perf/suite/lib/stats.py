"""Small-sample statistics: Student-t 95% intervals (3 runs need 4.30 standard errors, not 1.96)."""

import math

# Two-sided 95% critical values of Student's t, by degrees of freedom.
_T95 = {
    1: 12.706, 2: 4.303, 3: 3.182, 4: 2.776, 5: 2.571, 6: 2.447, 7: 2.365, 8: 2.306,
    9: 2.262, 10: 2.228, 11: 2.201, 12: 2.179, 13: 2.160, 14: 2.145, 15: 2.131,
    16: 2.120, 17: 2.110, 18: 2.101, 19: 2.093, 20: 2.086, 21: 2.080, 22: 2.074,
    23: 2.069, 24: 2.064, 25: 2.060, 26: 2.056, 27: 2.052, 28: 2.048, 29: 2.045,
    30: 2.042,
}


def t_crit(df):
    if df < 1:
        return None
    if df in _T95:
        return _T95[df]
    for limit, value in ((40, 2.021), (60, 2.000), (120, 1.980)):
        if df <= limit:
            return value
    return 1.960


def mean_ci(xs):
    """(mean, half-width of the 95% CI, n). Half-width is None when n < 2."""
    xs = [x for x in xs if x is not None]
    n = len(xs)
    if n == 0:
        return (None, None, 0)
    mean = sum(xs) / n
    if n < 2:
        return (mean, None, n)
    var = sum((x - mean) ** 2 for x in xs) / (n - 1)
    return (mean, t_crit(n - 1) * math.sqrt(var / n), n)


def paired_diff(a, b):
    """CI of the per-pair differences a_i - b_i; pairing cancels drift shared within a block."""
    pairs = [(x, y) for x, y in zip(a, b) if x is not None and y is not None]
    return mean_ci([x - y for x, y in pairs])


def fmt_ci(mean, half, digits=2):
    if mean is None:
        return "n/a"
    if half is None:
        return f"{mean:.{digits}f}"
    return f"{mean:.{digits}f} ±{half:.{digits}f}"


def loglog_slope(points):
    """Slope of log(y) on log(x): ~1 linear, ~0 independent. Non-positive points are dropped."""
    pts = [(math.log(x), math.log(y)) for x, y in points if x and y and x > 0 and y > 0]
    if len(pts) < 2:
        return None
    mx = sum(p[0] for p in pts) / len(pts)
    my = sum(p[1] for p in pts) / len(pts)
    den = sum((p[0] - mx) ** 2 for p in pts)
    if den == 0:
        return None
    return sum((p[0] - mx) * (p[1] - my) for p in pts) / den
