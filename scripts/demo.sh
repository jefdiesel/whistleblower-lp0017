#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Whistleblower (LP-0017) — REAL end-to-end demo with on-chain anchoring.
#
# This is the demo the narrated video records. It assumes the surrounding
# infrastructure is ALREADY running (see HANDOFF.md):
#
#   * Logos Storage (Codex) node     -> REST :8080  (scripts/run-nodes.sh)
#   * Logos Delivery (Waku) node     -> REST :8645  (scripts/run-nodes.sh)
#   * LEZ standalone sequencer       -> JSON-RPC :3040, RISC0_DEV_MODE=0
#                                       (scripts/run-sequencer.sh)
#   * the whistleblower_registry program is DEPLOYED, and its program id is
#     exported as WB_PROGRAM_ID; a signing key is exported as WB_SIGNER_KEY.
#
# Flow:
#   1. wb-batch-anchor        publish <file>   -> upload to Storage (CID) + broadcast
#   2. wb-batch-anchor-lez    anchor           -> drain the topic, anchor the CID
#                                                 on-chain (REAL RISC0 proof)
#   3. wb-batch-anchor-lez    query <cid>      -> confirm the CID is registered
#
# RISC0_DEV_MODE=0 is forced: step 2 generates a real zero-knowledge proof. On
# Apple Silicon (no CUDA GPU) expect this to take seconds-to-minutes — that wait
# is the proof being generated and is exactly what the video should capture.

set -euo pipefail
export RISC0_DEV_MODE=0

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

# --- configuration (override via env) ----------------------------------------
SAMPLE_FILE="${SAMPLE_FILE:-${SCRIPT_DIR}/sample-doc.txt}"
TITLE="${TITLE:-LP-0017 sample disclosure}"
DESCRIPTION="${DESCRIPTION:-End-to-end demo document for the Whistleblower registry.}"
TAGS="${TAGS:-leak,demo,lp-0017}"

export WB_STORAGE_URL="${WB_STORAGE_URL:-http://localhost:8080/api/storage/v1}"
export WB_DELIVERY_URL="${WB_DELIVERY_URL:-http://127.0.0.1:8645}"
export WB_SEQUENCER_URL="${WB_SEQUENCER_URL:-http://localhost:3040}"
export WB_TOPIC="${WB_TOPIC:-/whistleblower/1/documents/json}"
export WB_CHECKPOINT="${WB_CHECKPOINT:-${ROOT_DIR}/.wb/demo-checkpoint.json}"
# WB_PROGRAM_ID and WB_SIGNER_KEY MUST be set in the environment (see HANDOFF.md).
# -----------------------------------------------------------------------------

# Locations of the two binaries. wb-batch-anchor is in the root workspace;
# wb-batch-anchor-lez is built from the excluded wb-lez-registry crate (its own
# workspace) on the provisioned machine.
WB="${WB:-${ROOT_DIR}/target/release/wb-batch-anchor}"
WB_LEZ="${WB_LEZ:-${ROOT_DIR}/crates/wb-lez-registry/target/release/wb-batch-anchor-lez}"

line() { printf '\n========================================================================\n'; }
need() { command -v "$1" >/dev/null 2>&1 || { echo "missing required tool: $1" >&2; exit 1; }; }

# --- preflight ---------------------------------------------------------------
line
echo "Whistleblower REAL end-to-end demo (RISC0_DEV_MODE=${RISC0_DEV_MODE})"
echo "  sample file:   ${SAMPLE_FILE}"
echo "  storage:       ${WB_STORAGE_URL}"
echo "  delivery:      ${WB_DELIVERY_URL}"
echo "  sequencer:     ${WB_SEQUENCER_URL}"
echo "  topic:         ${WB_TOPIC}"
line

[[ -f "${SAMPLE_FILE}" ]] || { echo "sample file not found: ${SAMPLE_FILE}" >&2; exit 1; }
[[ -x "${WB}" ]] || { echo "wb-batch-anchor not built at ${WB}"; echo "  build it: cargo build --release -p wb-batch-anchor"; exit 1; }
[[ -x "${WB_LEZ}" ]] || { echo "wb-batch-anchor-lez not built at ${WB_LEZ}"; echo "  build it (provisioned machine): cd crates/wb-lez-registry && cargo build --release"; exit 1; }
: "${WB_PROGRAM_ID:?WB_PROGRAM_ID must be set (the deployed whistleblower_registry program id; = RISC0 image id)}"
: "${WB_SIGNER_KEY:?WB_SIGNER_KEY must be set (hex signing key authorizing the anchor tx)}"

# ---------------------------------------------------------------------------
line
echo "[1/3] PUBLISH — upload bytes to Logos Storage, broadcast the envelope"
echo "      This uploads ${SAMPLE_FILE} to Codex (returns a CID) and broadcasts"
echo "      a JSON metadata envelope over Waku to ${WB_TOPIC}."
line
PUBLISH_OUT="$("${WB}" publish "${SAMPLE_FILE}" \
  --title "${TITLE}" \
  --description "${DESCRIPTION}" \
  --tags "${TAGS}")"
echo "${PUBLISH_OUT}"

# Extract the CID from the "  CID:  <cid>" line of publish output.
CID="$(printf '%s\n' "${PUBLISH_OUT}" | awk '/^[[:space:]]*CID:/ {print $2; exit}')"
[[ -n "${CID}" ]] || { echo "could not parse CID from publish output" >&2; exit 1; }
echo
echo "Published CID: ${CID}"

# ---------------------------------------------------------------------------
line
echo "[2/3] ANCHOR — drain the topic and anchor the CID ON-CHAIN (REAL proof)"
echo "      wb-batch-anchor-lez subscribes to ${WB_TOPIC}, picks up the CID just"
echo "      broadcast, builds the anchor_batch transaction, and submits it to the"
echo "      LEZ sequencer. With RISC0_DEV_MODE=0 the sequencer generates a real"
echo "      RISC0 zkVM proof — THIS is the step the video should show taking time."
line
"${WB_LEZ}" anchor

# ---------------------------------------------------------------------------
line
echo "[3/3] QUERY — confirm the CID is registered on-chain"
echo "      Derives the PDA from the CID (SHA256(\"WB-CID-PDA-v1\"||cid)), reads"
echo "      the account back, and decodes the RegistryRecord."
line
"${WB_LEZ}" query "${CID}"
echo
echo "Same query, --json:"
"${WB_LEZ}" query "${CID}" --json

line
echo "REAL END-TO-END DEMO COMPLETE."
echo "  file -> Storage[CID=${CID}] -> Delivery[envelope] -> on-chain registry[PDA]"
echo "  and the record is queryable by CID."
line
