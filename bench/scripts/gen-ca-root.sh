#!/bin/sh
# gen-ca-root.sh
#
# Generates Ed25519 PKI certificates for all Atlas nodes into $GENERATED/ca-root/
# using the keygen crate from Atlas-Tools.
# Supports two layouts controlled by CA_ROOT_FORMAT (default: flattened):
#
#   flattened  — FlattenedPathConstructor layout (atlas-default-configs):
#                  ca-root/ca-root-{id}-private   (PKCS8 DER)
#                  ca-root/ca-root-{id}-private_pem
#                  ca-root/ca-root-{id}-cert
#                  ca-root/ca-root-{id}-public
#                  ca-root/ca-root-{id}-public_pcks
#                  ca-root/ca-root-cert            (root CA cert)
#                  ca-root/ca-root-private         (root CA private key, PKCS8 DER)
#                  ca-root/ca-root-private_pem
#                  ca-root/ca-root-public_pcks
#
#   folder     — FolderPathConstructor layout (keygen tool native output):
#                  ca-root/{id}/private
#                  ca-root/{id}/private_pem
#                  ca-root/{id}/cert
#                  ca-root/{id}/public
#                  ca-root/{id}/public_pcks
#                  ca-root/cert                    (root CA cert)
#                  ca-root/private                 (root CA private key, PKCS8 DER)
#                  ca-root/private_pem
#                  ca-root/public_pcks
#
# Node IDs:
#   Replicas: 0 .. N_REPLICAS-1
#   Clients:  1000 .. 1000+N_CLIENTS-1  (CLI_BASE=1000)
#
# Reads from environment (exported by the bench Makefile):
#   GENERATED           - absolute path to shared generated/ dir
#   N_REPLICAS          - number of replica nodes
#   N_CLIENTS           - number of client nodes
#   GLOBAL_BENCH_DIR    - absolute path to testing-grounds/bench/
#   CA_ROOT_FORMAT      - flattened (default) | folder
#
# Skips if ca-root already exists.
# Run `make regen-ca-root` to force regeneration (e.g. after changing N_REPLICAS).

set -eu

: "${GENERATED:?GENERATED not set}"
: "${N_REPLICAS:?N_REPLICAS not set}"
: "${N_CLIENTS:?N_CLIENTS not set}"
: "${GLOBAL_BENCH_DIR:?GLOBAL_BENCH_DIR not set}"
CA_ROOT_FORMAT="${CA_ROOT_FORMAT:-flattened}"

CA_ROOT="$GENERATED/ca-root"
CLI_BASE=1000

if [ "$CA_ROOT_FORMAT" != "flattened" ] && [ "$CA_ROOT_FORMAT" != "folder" ]; then
    echo "ERROR: CA_ROOT_FORMAT must be 'flattened' or 'folder' (got '$CA_ROOT_FORMAT')" >&2
    exit 1
fi

if [ -d "$CA_ROOT" ]; then
    echo "ca-root already exists at $CA_ROOT — skipping. Run 'make regen-ca-root' to regenerate."
    exit 0
fi

# ── Locate and build the keygen crate ─────────────────────────────────────────
# GLOBAL_BENCH_DIR is testing-grounds/bench; two levels up is the project root,
# and the Atlas repo lives at Atlas/ within it.
ATLAS_ROOT="$(cd "$GLOBAL_BENCH_DIR/../.." && pwd)"
KEYGEN_DIR="$ATLAS_ROOT/Atlas/Atlas-Tools/keygen"

if [ ! -d "$KEYGEN_DIR" ]; then
    echo "ERROR: keygen crate not found at $KEYGEN_DIR" >&2
    exit 1
fi

echo "Building keygen..."
cargo build --release --manifest-path "$KEYGEN_DIR/Cargo.toml"

KEYGEN_BIN="$KEYGEN_DIR/target/release/keygen"

# ── Generate certificates ──────────────────────────────────────────────────────
# keygen always writes the folder layout; for flattened we use a temp dir.
if [ "$CA_ROOT_FORMAT" = "folder" ]; then
    GEN_DIR="$CA_ROOT"
    mkdir -p "$GEN_DIR"
else
    GEN_DIR="$(mktemp -d)"
fi

echo "Generating ca-root (format: $CA_ROOT_FORMAT)..."

"$KEYGEN_BIN" \
    --output-dir "$GEN_DIR" \
    --replica-count "$N_REPLICAS" \
    --client-count "$N_CLIENTS" \
    --first-replica-id 0 \
    --first-client-id "$CLI_BASE" \
    ed25519

# ── Flatten if requested ───────────────────────────────────────────────────────
if [ "$CA_ROOT_FORMAT" = "flattened" ]; then
    mkdir -p "$CA_ROOT"

    # Root CA files
    cp "$GEN_DIR/cert"        "$CA_ROOT/ca-root-cert"
    cp "$GEN_DIR/private"     "$CA_ROOT/ca-root-private"
    cp "$GEN_DIR/private_pem" "$CA_ROOT/ca-root-private_pem"
    cp "$GEN_DIR/public_pcks" "$CA_ROOT/ca-root-public_pcks"

    # Per-node files
    for id_dir in "$GEN_DIR"/*/; do
        id=$(basename "$id_dir")
        for ft in private private_pem public public_pcks cert; do
            src="$id_dir$ft"
            [ -f "$src" ] && cp "$src" "$CA_ROOT/ca-root-${id}-${ft}"
        done
    done

    rm -rf "$GEN_DIR"
fi

echo "Done — ca-root generated at $CA_ROOT ($N_REPLICAS replicas, $N_CLIENTS clients, format: $CA_ROOT_FORMAT)"
