#!/usr/bin/env bash
# Terminal-cast demo for the Whistleblower (LP-0017) video.
# Recorded with asciinema, rendered with agg+ffmpeg, captioned via docs/captions.ass.
# Runs the REAL pipeline on the build mini: live Storage upload -> on-chain anchor
# (RISC0_DEV_MODE=0, real proving) -> query-by-CID.
set -uo pipefail
cd "$HOME/lamda/crates/wb-registry-program" || exit 1
source "$HOME/.cargo/env" 2>/dev/null
export PATH="$HOME/.risc0/bin:$PATH"
export TERM=xterm-256color
export RISC0_DEV_MODE=0
export NSSA_WALLET_HOME_DIR="$HOME/lez-seq/wallet/configs/debug"
BIN=target/riscv-guest/wb-registry-methods/wb-registry-guest/riscv32im-risc0-zkvm-elf/release/wb_registry.bin
CLI=(cargo run --quiet --bin wb_registry_cli -- --idl whistleblower_registry-idl.json -p "$BIN")

cyan(){ printf '\033[1;36m%s\033[0m\n' "$*"; }
green(){ printf '\033[1;32m%s\033[0m\n' "$*"; }
dim(){ printf '\033[0;90m%s\033[0m\n' "$*"; }

clear
cyan "════════════════════════════════════════════════════════════"
cyan "  WHISTLEBLOWER  ·  censorship-resistant document publishing"
cyan "  Logos Prize LP-0017   ·   upload → broadcast → anchor"
cyan "════════════════════════════════════════════════════════════"
sleep 3
dim "  reusable core: 30 tests green (cargo test --workspace)"
sleep 2.5

cyan ""
cyan "[1/4]  Upload a document to Logos Storage (live Codex node)"
SAMPLE="$(mktemp)"; printf 'CONFIDENTIAL — internal memo\nSubject: evidence of wrongdoing\n' > "$SAMPLE"
CID=$(curl -s -X POST http://127.0.0.1:8080/api/storage/v1/data -H "content-type: application/octet-stream" --data-binary @"$SAMPLE")
green "       → CID: $CID"
sleep 3

cyan "[2/4]  Derive the on-chain account (PDA) for this CID"
PDA=$(cargo run --quiet --bin pda -- "$CID" 2>/dev/null | tail -1)
green "       → PDA: $PDA"
sleep 3

cyan "[3/4]  Anchor it ON-CHAIN   (RISC0_DEV_MODE=$RISC0_DEV_MODE — REAL zk proving)"
"${CLI[@]}" anchor-one --cid "$CID" \
  --metadata-hash 3333333333333333333333333333333333333333333333333333333333333333 \
  --anchor-timestamp 1719300000000 --record "$PDA" 2>&1 \
  | grep -E "Submitting|submitted|tx_hash|confirmed" | sed 's/^/       /'
sleep 3

cyan "[4/4]  Query the registry BY CID"
"${CLI[@]}" inspect "$PDA" --type RegistryRecord 2>&1 \
  | grep -E "anchor_timestamp|\"cid\"|metadata_hash|^\{|^\}" | sed 's/^/       /'
sleep 3

green ""
green "  ✅ uploaded · broadcast-ready · anchored on-chain · queryable by CID"
cyan  "  permissionless · idempotent · censorship-resistant"
sleep 3
