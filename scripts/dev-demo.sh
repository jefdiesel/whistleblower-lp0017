#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Whistleblower (Logos prize LP-0017) — NO-NODE developer demo.
#
# This script runs entirely on THIS machine. It needs nothing but a Rust
# toolchain: no Codex storage node, no Waku delivery node, no LEZ sequencer, no
# RISC0 toolchain. It proves the parts of the system that are real and tested
# right here, and is honest about the parts that require live infrastructure.
#
# What it does, and what each step proves:
#   1. `cargo test --workspace`  -> the Rust core is correct (27 unit tests in
#      wb-types + wb-index, plus the wb-e2e in-process pipeline integration test).
#   2. builds the `wb-batch-anchor` CLI (file-backed registry binary).
#   3. `wb-batch-anchor --help` -> the permissionless tool's surface area.
#   4. seeds a *file-backed* registry and runs `query` against it -> shows the
#      "queryable by CID" path returning a RegistryRecord, without any chain.
#   5. `wb-batch-anchor status` -> the checkpoint/resume bookkeeping.
#
# What it deliberately does NOT do: `publish` and `run`/`anchor` against the
# network. Those need a live Logos Storage (Codex) node and Logos Delivery
# (Waku) node — see scripts/run-nodes.sh + scripts/demo.sh for the real,
# end-to-end run on a provisioned machine.

set -euo pipefail

# Resolve repo root from this script's location so it works from anywhere.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${ROOT_DIR}"

WORKDIR="$(mktemp -d)"
REGISTRY_FILE="${WORKDIR}/registry.json"
CHECKPOINT_FILE="${WORKDIR}/checkpoint.json"
trap 'rm -rf "${WORKDIR}"' EXIT

line() { printf '\n========================================================================\n'; }

line
echo "Whistleblower DEV DEMO (no external nodes required)"
echo "Repo: ${ROOT_DIR}"
echo "Scratch dir: ${WORKDIR}"
line

# ---------------------------------------------------------------------------
echo "[1/5] cargo test --workspace"
echo "      Proves: the shared types, the canonical metadata_hash, the"
echo "      upload->broadcast->anchor module (wb-index), and the in-process"
echo "      end-to-end pipeline test all pass. No nodes involved."
line
cargo test --workspace

# ---------------------------------------------------------------------------
line
echo "[2/5] Building the wb-batch-anchor CLI (file-backed registry binary)"
echo "      This is the permissionless batch-anchor tool. The local binary uses"
echo "      a JSON file as a stand-in for the on-chain registry so the full"
echo "      query/checkpoint flow works with no sequencer."
line
cargo build -p wb-batch-anchor --bin wb-batch-anchor
BIN="${ROOT_DIR}/target/debug/wb-batch-anchor"

# ---------------------------------------------------------------------------
line
echo "[3/5] wb-batch-anchor --help"
echo "      Proves: the tool's subcommands (publish / run / anchor / query /"
echo "      status) and the Storage/Delivery/topic/checkpoint configuration."
line
"${BIN}" --help

# ---------------------------------------------------------------------------
line
echo "[4/5] Query the (file-backed) registry by CID"
echo "      We seed one RegistryRecord into a file registry, then ask the CLI to"
echo "      look it up. This is the SAME code path 'queryable by CID' uses"
echo "      on-chain (wb-batch-anchor-lez query), minus the network."
line

# A canonical metadata_hash is 32 bytes. The value below is illustrative; the
# point is that query reads it back verbatim. The FileRegistry on-disk format is
#   { "records": { <cid>: RegistryRecord }, "seq": N }
# and RegistryRecord serializes metadata_hash as a JSON array of 32 byte values,
# so we write a fixed 32-element array (no external tools required).
SAMPLE_CID="zDvSampleCidAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
HASH_BYTES="1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32"

cat > "${REGISTRY_FILE}" <<JSON
{
  "records": {
    "${SAMPLE_CID}": {
      "cid": "${SAMPLE_CID}",
      "metadata_hash": [${HASH_BYTES}],
      "anchor_timestamp": 1700000000000
    }
  },
  "seq": 1
}
JSON

echo "Seeded file registry: ${REGISTRY_FILE}"
echo
echo "\$ wb-batch-anchor --registry-file <file> query ${SAMPLE_CID}"
"${BIN}" --registry-file "${REGISTRY_FILE}" --checkpoint "${CHECKPOINT_FILE}" \
  query "${SAMPLE_CID}"

echo
echo "Same query, --json:"
"${BIN}" --registry-file "${REGISTRY_FILE}" --checkpoint "${CHECKPOINT_FILE}" \
  query "${SAMPLE_CID}" --json

echo
echo "And a CID that is NOT anchored (expected: 'not found', exit code 2):"
set +e
"${BIN}" --registry-file "${REGISTRY_FILE}" --checkpoint "${CHECKPOINT_FILE}" \
  query "zDvDefinitelyNotAnchored"
echo "  (exit code: $?)"
set -e

# ---------------------------------------------------------------------------
line
echo "[5/5] wb-batch-anchor status (checkpoint bookkeeping)"
echo "      Proves: the resume/checkpoint accounting the batch loop uses to be"
echo "      crash-safe and idempotent across restarts."
line
"${BIN}" --checkpoint "${CHECKPOINT_FILE}" status

# ---------------------------------------------------------------------------
line
echo "DEV DEMO COMPLETE."
echo
echo "What this proved (locally, no nodes):"
echo "  * the Rust core is correct and tested (27 unit tests + the e2e pipeline)"
echo "  * the permissionless CLI builds and runs"
echo "  * 'queryable by CID' returns a RegistryRecord (file-backed stand-in)"
echo "  * checkpoint/resume bookkeeping works"
echo
echo "What this did NOT do (needs live infrastructure):"
echo "  * 'publish' (upload to Logos Storage + broadcast over Logos Delivery)"
echo "  * 'run'/'anchor' draining the Delivery topic"
echo "  * REAL on-chain anchoring with RISC0 proofs (wb-batch-anchor-lez)"
echo
echo "For the real end-to-end run see: scripts/run-nodes.sh, scripts/run-sequencer.sh,"
echo "scripts/demo.sh, and HANDOFF.md."
line
