"""Names for the uniform-RTT WAN profiles (shared by the generator, plan loader and runner)."""

DEFAULT_LEVELS_MS = [0.4, 5, 20, 50, 100, 200]


def label(rtt_ms):
    """0.4 -> '0p4ms', 50 -> '50ms'. Dots are not safe in a file or tag name."""
    return f"{rtt_ms:g}".replace(".", "p") + "ms"


def profile_name(rtt_ms):
    return f"uniform-rtt-{label(rtt_ms)}"
