#!/bin/bash
# run.sh — used by remote-bare deployment to start a node process.
# The shared .env (written by Ansible) exports BINARY_NAME, RUST_LOG, etc.
# The per-node .own_<id>.env exports OWN_NODE__* identity vars.

. .env
. ".own_${1}.env"

./${BINARY_NAME}
