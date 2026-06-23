# Whistleblower (LP-0017) — current status

_Snapshot of what is built, what is verified on real hardware, and what remains._

## ✅ Done and verified

**Reusable core (this laptop, `cargo test --workspace`): 30 tests green.**
- `wb-types` — metadata envelope + canonical, cross-language `metadata_hash` + on-chain record.
- `wb-index` — the reusable upload→broadcast→anchor module: HTTP Storage/Delivery
  clients, retrying upload, dedup broadcast, idempotent registry, checkpoint/resume
  batch runner. (6 + 21 unit tests; 3 end-to-end pipeline tests.)
- `wb-batch-anchor` — permissionless CLI (`publish`/`run`/`anchor`/`query`/`status`).

**On-chain registry — compiled, deployed, and exercised on real hardware**
(standalone LEZ sequencer on an Apple M4, **`RISC0_DEV_MODE=0`** = real proving):
- SPEL `#[lez_program]` compiles to a riscv32 zkVM binary; IDL generated
  (`whistleblower_registry-idl.json`, lists `anchor_batch` + `anchor_one`).
- Program image id (on-chain address):
  `[736378292, 3237769127, 3218962078, 1003268346, 3355061132, 654317770, 4171522436, 2002532608]`.
- **Deployed** (`wallet deploy-program`) — deploy tx `cc40044d…` included in block 85, no "Malformed".
- **Real anchor** — `anchor-one` tx `32673a9feb866e56f3fb0ac7ad8136804cd82fd07f712953bc4c0318c0c17213`,
  correlated to the sequencer log line "Validated transaction … including it in block".
- **Query by CID** — `inspect` on PDA `76d5eUdWWRBf7SbcSQFHEmzD4bJKAjqbK3mAqwecR1Tg`
  read back `RegistryRecord { cid: zDvTestWhistleblowerCID0001, metadata_hash: 0x11×32, anchor_timestamp: 1719100000000 }`.
  Full submit → in-block confirm → read-back-by-CID round trip.
- **Batch anchor (≥10 and 50-CID) — verified end-to-end via `wb-lez-registry`.**
  A 12-CID `anchor_batch` in ONE tx (`79790d08…`) wrote **12/12** PDAs (each read
  back by CID), idempotent on re-run; a **50-CID** batch wrote **50/50**, across
  three fresh runs. Root-caused + fixed the earlier "tx accepted but nothing
  persisted" bug: the instruction is risc0 word-serde of the SPEL `Instruction`
  enum (not borsh / not a SHA256 discriminator), and the signer-less IDL requires
  an **empty witness** — a fee-payer signature prepended an account and tripped the
  guest's `records.len() == entries.len()` check. Verified against LEZ v0.1.2 source.
- **Benchmarks (M4, `RISC0_DEV_MODE=0`, real on-chain):** single-CID **~3.0 ms**,
  50-CID **~48.6 ms** = **0.97 ms/CID** (batching ~3× cheaper per CID). See
  `docs/benchmarks.md`.

**Logos Storage node — real, on the mini:** `logos-storage` v0.4.0-rc1 running
(`:8080`, base path `/api/storage/v1`); a real upload returned CID
`zDvZRwzkzV13MTPdp2bAvNSr3gkUMseQd1D1BhoZPavYYcdx2BTa`.

**Other deliverables present:** Basecamp Qt/QML app (`app/`); the on-chain
`RegistryClient` adapter + `wb-batch-anchor-lez` binary (`crates/wb-lez-registry/`,
**compiled and batch-verified on the mini** against the real LEZ v0.1.2 API); CI
workflow; demo scripts; READMEs;
`HANDOFF.md`; this video script (`docs/VIDEO-SCRIPT.md`).

## ◑ Partial / in progress

- **Logos Delivery node** — no prebuilt macOS-arm64 binary exists and Colima had no
  public-internet egress on the mini, so the upload→broadcast beat uses the
  in-process dev path. The broadcast code is real (`wb-index` `HttpDelivery`) and
  unit-tested; it just hasn't been exercised against a live Waku node. (Issue #6.)
- **RISC0 user/total cycle counts** — a nicety. The headline cost numbers
  (executor wall-time, above) are captured; isolated cycle totals would need a
  small executor-only `cycle_bench` harness.

## ☐ Remaining (incl. human-only)

- Record the **narrated `RISC0_DEV_MODE=0` video** (script: `docs/VIDEO-SCRIPT.md`).
- Publish the **public repo** and open the **solution PR** to `logos-co/lambda-prize`.
- **Public LEZ testnet (decision: documented as a gap, not attempted).** There is
  no public LEZ *sequencer* endpoint — only the L1/Bedrock node testnet is
  published. Our deployment is a real **standalone** LEZ sequencer (mock L1
  settlement) with a documented image id, and anchors run with `RISC0_DEV_MODE=0`.
  A full public-testnet LEZ would require an L1 (Bedrock/cryptarchia) node + an
  Indexer + a non-standalone sequencer wired to both; filed as issue #4.

## Note on evidence location

The on-chain evidence (tx hashes, `~/seq.log`, the deployed `.bin`) lives on the
build mini, not in this checkout. Reproduce from a provisioned host via `HANDOFF.md`.
