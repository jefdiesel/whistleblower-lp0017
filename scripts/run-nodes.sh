#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Whistleblower (LP-0017) — start the local infrastructure nodes.
#
#   * Logos Storage node  = Codex   -> REST on :8080
#   * Logos Delivery node = nwaku   -> REST on :8645
#
# These are BEST-EFFORT commands. Image names, tags, and flags drift between
# releases; adjust the marked variables to match what is published when you run
# this. The goal is to expose:
#
#   Storage (Codex):  http://localhost:8080
#       base path on current nodes:  /api/storage/v1
#       (some published SDKs use:     /api/codex/v1  -- see "Issues filed" in README)
#   Delivery (Waku):  http://127.0.0.1:8645   (REST/relay autosharding API)
#
# wb-batch-anchor / wb-batch-anchor-lez default to exactly those endpoints:
#   --storage-url  http://localhost:8080/api/storage/v1   (env WB_STORAGE_URL)
#   --delivery-url http://127.0.0.1:8645                   (env WB_DELIVERY_URL)
#
# Run this in its own terminal; it launches both nodes in the foreground via
# Docker by default. Ctrl-C stops them.

set -euo pipefail

# --- ADJUST THESE to the images/tags you have available ----------------------
CODEX_IMAGE="${CODEX_IMAGE:-codexstorage/nim-codex:latest}"
WAKU_IMAGE="${WAKU_IMAGE:-wakuorg/nwaku:latest}"
STORAGE_REST_PORT="${STORAGE_REST_PORT:-8080}"
DELIVERY_REST_PORT="${DELIVERY_REST_PORT:-8645}"
DATA_DIR="${DATA_DIR:-$(pwd)/.wb-nodes}"
# Waku content topic the app/CLI use (the node does not strictly need this, but
# some setups pin a pubsub/content topic at launch).
WB_TOPIC="${WB_TOPIC:-/whistleblower/1/documents/json}"
# -----------------------------------------------------------------------------

mkdir -p "${DATA_DIR}/codex" "${DATA_DIR}/waku"

have() { command -v "$1" >/dev/null 2>&1; }

if ! have docker; then
  cat <<'EOF'
docker not found.

Run the nodes from native binaries instead, exposing the same ports:

  # --- Logos Storage (Codex), REST on :8080, base path /api/storage/v1 -------
  codex \
    --data-dir=./.wb-nodes/codex \
    --api-port=8080 \
    --api-bindaddr=0.0.0.0 \
    --disc-port=8090 \
    --listen-addrs=/ip4/0.0.0.0/tcp/8070
  # (On older builds the API prefix is /api/codex/v1; on current nodes it is
  #  /api/storage/v1. Point WB_STORAGE_URL at whichever your node serves.)

  # --- Logos Delivery (nwaku/Waku), REST on :8645 ----------------------------
  nwaku-node \
    --rest=true --rest-port=8645 --rest-address=0.0.0.0 \
    --relay=true \
    --pubsub-topic="/waku/2/rs/0/0" \
    --nat=none
  # The app/CLI publish to content topic /whistleblower/1/documents/json under
  # the relay autosharding API (POST /relay/v1/auto/messages/<topic>).

Then in another terminal:  scripts/run-sequencer.sh  and  scripts/demo.sh
EOF
  exit 1
fi

cleanup() {
  echo
  echo "Stopping nodes…"
  docker rm -f wb-codex wb-waku >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

echo "Starting Logos Storage (Codex) on :${STORAGE_REST_PORT}  image=${CODEX_IMAGE}"
docker run -d --name wb-codex \
  -p "${STORAGE_REST_PORT}:8080" \
  -v "${DATA_DIR}/codex:/data" \
  "${CODEX_IMAGE}" \
  --data-dir=/data \
  --api-port=8080 \
  --api-bindaddr=0.0.0.0 \
  >/dev/null

echo "Starting Logos Delivery (nwaku/Waku) on :${DELIVERY_REST_PORT}  image=${WAKU_IMAGE}"
docker run -d --name wb-waku \
  -p "${DELIVERY_REST_PORT}:8645" \
  -v "${DATA_DIR}/waku:/data" \
  "${WAKU_IMAGE}" \
  --rest=true \
  --rest-port=8645 \
  --rest-address=0.0.0.0 \
  --relay=true \
  --nat=none \
  >/dev/null

cat <<EOF

Nodes are starting. Verify they are up:

  # Storage (Codex) — expect a JSON node/debug info object:
  curl -s http://localhost:${STORAGE_REST_PORT}/api/storage/v1/debug/info || \
  curl -s http://localhost:${STORAGE_REST_PORT}/api/codex/v1/debug/info

  # Delivery (Waku) — expect HTTP 200 from the relay subscriptions endpoint:
  curl -s -o /dev/null -w '%{http_code}\n' \
    -X POST http://127.0.0.1:${DELIVERY_REST_PORT}/relay/v1/auto/subscriptions \
    -H 'content-type: application/json' \
    -d '["${WB_TOPIC}"]'

Endpoints for the CLI:
  export WB_STORAGE_URL=http://localhost:${STORAGE_REST_PORT}/api/storage/v1
  export WB_DELIVERY_URL=http://127.0.0.1:${DELIVERY_REST_PORT}

Following node logs (Ctrl-C to stop both)…
EOF

docker logs -f wb-codex &
docker logs -f wb-waku &
wait
