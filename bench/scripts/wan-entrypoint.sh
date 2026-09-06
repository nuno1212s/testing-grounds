#!/bin/sh
# wan-entrypoint.sh — baked into the `final-wan` runtime image.
#
# Applies this node's tc/netem shaping spec (bind-mounted at /wan) inside the
# container's own network namespace, then execs the server. Running the shaping
# here — rather than from a sidecar — means the rules are in place *before* the
# first packet the server sends, so there is no unshaped connection-setup window.
#
# Environment:
#   WAN_NODE  - node name; selects /wan/<WAN_NODE>.sh   (unset => no shaping)
#   WAN_IFACE - interface to shape (default eth0)
#
# The spec is bind-mounted rather than baked in, so `make <project> wan-apply`
# can regenerate and re-apply rules without rebuilding the image.

set -e

IFACE="${WAN_IFACE:-eth0}"
SPEC="/wan/${WAN_NODE:-__unset__}.sh"

if [ -n "${WAN_NODE:-}" ] && [ -r "$SPEC" ]; then
    echo "[wan] applying $SPEC on $IFACE"
    # Fail hard: a benchmark that silently ran unshaped is worse than one that
    # refuses to start.
    if ! sh "$SPEC"; then
        echo "[wan] FAILED to apply shaping from $SPEC" >&2
        echo "[wan] does this container have CAP_NET_ADMIN and does the kernel have sch_netem?" >&2
        exit 1
    fi
    tc -s qdisc show dev "$IFACE"
else
    echo "[wan] no spec for '${WAN_NODE:-<unset>}' — running unshaped"
fi

exec "$@"
