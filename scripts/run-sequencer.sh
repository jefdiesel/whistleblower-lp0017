#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Whistleblower (LP-0017) — run the LEZ standalone sequencer.
#
# The Logos Execution Zone (LEZ) standalone sequencer accepts transactions over
# JSON-RPC (default :3040) and, when RISC0_DEV_MODE=0, generates REAL RISC0 zkVM
# proofs for the programs it executes — including the `whistleblower_registry`
# program's `anchor_batch`. wb-batch-anchor-lez submits to it via WB_SEQUENCER_URL
# (default http://localhost:3040).
#
# Prerequisites (provisioned machine only — see HANDOFF.md):
#   * Rust toolchain (rust-toolchain.toml -> 1.94.0)
#   * RISC0 toolchain:  curl -L https://risczero.com/install | bash && rzup install
#   * A local checkout of logos-execution-zone at tag v0.1.2 (the LEZ_DIR below),
#     with the issue #468 `ring` patch applied (risc0-zkvm default-features=false)
#     so the build graph cross-compiles.
#
# This script does NOT vendor LEZ; point LEZ_DIR at your checkout.

set -euo pipefail

# --- ADJUST THESE to your LEZ checkout ---------------------------------------
# Where you cloned https://github.com/logos-blockchain/logos-execution-zone
# (checked out at tag v0.1.2). HANDOFF.md suggests ./vendor/logos-execution-zone.
LEZ_DIR="${LEZ_DIR:-$(pwd)/vendor/logos-execution-zone}"
# The sequencer config shipped with LEZ for local/debug runs.
SEQ_CONFIG="${SEQ_CONFIG:-configs/debug/sequencer_config.json}"
# JSON-RPC port the sequencer listens on (informational; set in the config).
SEQ_PORT="${SEQ_PORT:-3040}"
# -----------------------------------------------------------------------------

# 0 = real proofs (REQUIRED for the LP-0017 demo). Override only for fast smoke
# tests; the narrated video MUST run with RISC0_DEV_MODE=0.
export RISC0_DEV_MODE="${RISC0_DEV_MODE:-0}"
export RUST_LOG="${RUST_LOG:-info}"

if [[ ! -d "${LEZ_DIR}" ]]; then
  cat <<EOF
LEZ checkout not found at: ${LEZ_DIR}

Clone and pin it (then re-run, or set LEZ_DIR):

  git clone https://github.com/logos-blockchain/logos-execution-zone "${LEZ_DIR}"
  git -C "${LEZ_DIR}" checkout v0.1.2
  # Apply the issue #468 ring patch: in the checkout, set
  #   risc0-zkvm = { version = "3.0.5", default-features = false, features = ["std"] }
  # everywhere risc0-zkvm appears.

See HANDOFF.md for the full sequence.
EOF
  exit 1
fi

if [[ ! -f "${LEZ_DIR}/${SEQ_CONFIG}" ]]; then
  echo "Sequencer config not found: ${LEZ_DIR}/${SEQ_CONFIG}" >&2
  echo "Look under ${LEZ_DIR}/configs/ for the standalone/debug config and set SEQ_CONFIG." >&2
  exit 1
fi

cat <<EOF
Starting LEZ standalone sequencer
  LEZ checkout:    ${LEZ_DIR}
  config:          ${SEQ_CONFIG}
  RISC0_DEV_MODE:  ${RISC0_DEV_MODE}   (0 = real proofs)
  JSON-RPC:        http://localhost:${SEQ_PORT}

(First build with the RISC0 toolchain present can take a while.)
EOF

cd "${LEZ_DIR}"
# The documented standalone invocation:
exec cargo run --features standalone -p sequencer_service "${SEQ_CONFIG}"
