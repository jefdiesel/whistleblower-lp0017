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

**Logos Storage node — real, on the mini:** `logos-storage` v0.4.0-rc1 running
(`:8080`, base path `/api/storage/v1`); a real upload returned CID
`zDvZRwzkzV13MTPdp2bAvNSr3gkUMseQd1D1BhoZPavYYcdx2BTa`.

**Other deliverables present:** Basecamp Qt/QML app (`app/`); the on-chain
`RegistryClient` adapter + `wb-batch-anchor-lez` binary (`crates/wb-lez-registry/`,
rewritten against the real LEZ v0.1.2 API); CI workflow; demo scripts; READMEs;
`HANDOFF.md`; this video script (`docs/VIDEO-SCRIPT.md`).

## ◑ Partial / in progress

- **Logos Delivery node** — no prebuilt macOS-arm64 binary exists; standing up
  Docker on the mini (Colima) to run the official nwaku image for the
  upload→broadcast demo beat.
- **CU/cycle benchmarks** (`docs/benchmarks.md`) — single-CID executor time
  measured (~3 ms on the mini); RISC0 cycle counts + 50-CID batch row still TBD.
- **`wb-lez-registry`** adapter is written/verified against the API but not yet
  compiled end-to-end (needs the RISC0 toolchain build on the mini).

## ☐ Remaining (incl. human-only)

- Full upload→broadcast→batch-anchor demo against the live Storage + Delivery nodes.
- Record the **narrated `RISC0_DEV_MODE=0` video** (script: `docs/VIDEO-SCRIPT.md`).
- Publish the **public repo** and open the **solution PR** to `logos-co/lambda-prize`.
- 50-CID batch on-chain: the IDL CLI can't encode `Vec<struct>`, so the batch path
  goes through `wb-lez-registry` / `BatchAnchorRunner` (single-CID `anchor_one`
  is what the CLI submits live).

## Note on evidence location

The on-chain evidence (tx hashes, `~/seq.log`, the deployed `.bin`) lives on the
build mini, not in this checkout. Reproduce from a provisioned host via `HANDOFF.md`.
