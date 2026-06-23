<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Whistleblower (LP-0017) — Submission status

An honest, per-criterion map of **every** LP-0017 success criterion to its status
and where it lives. Status values:

- **DONE** — implemented *and* exercised (tests pass, or run on real hardware).
- **PARTIAL** — code complete and the host-side behaviour is tested, but full
  satisfaction needs a remaining step (a build or a live Delivery node — see notes).
- **REMAINING** — not yet done (a provisioned-machine step or a human action).

> **Verified on real hardware.** The on-chain program **is** compiled to a
> `riscv32im-risc0-zkvm-elf` zkVM binary, **deployed**, and exercised through a
> real **anchor → in-block confirm → query-by-CID** round trip on a **standalone
> LEZ sequencer** with **`RISC0_DEV_MODE=0`** (real proving) on an Apple M4.
> Evidence: program image id `[736378292, 3237769127, 3218962078, 1003268346,
> 3355061132, 654317770, 4171522436, 2002532608]`; deploy tx `cc40044d…`;
> repeatable single-CID anchor (e.g. tx `c426cd03…cb991`) correlated to the
> sequencer's "Validated transaction … including it in block"; `inspect` decoded
> the stored `RegistryRecord` back. The Logos **Storage** node is also live
> (real Codex upload returned a CID). See `docs/STATUS.md`.
>
> Honest caveats: (1) the **batch** on-chain path (`anchor_batch(Vec<…>)`) runs
> through the `crates/wb-lez-registry` adapter, now **compiled and verified
> on-chain** — a **12-CID** batch (tx `79790d08…`) and a **50-CID** batch each
> landed in one transaction with every record read back by CID
> (`RISC0_DEV_MODE=0`); (2) a real Waku
> **Delivery** node could not run on this Mac (no macOS-arm64 binary; the local
> Docker VM has no public-internet egress), so the broadcast beat is shown via
> the in-process dev path and runs trivially on any Linux box; (3) the on-chain
> evidence lives on the build mini, reproducible per `HANDOFF.md`.

Verified here: `cargo test --workspace` (30 tests) and `bash scripts/dev-demo.sh`
(no-node demo); on-chain via the live sequencer on the mini.

---

## Functionality

| # | Criterion | Status | Note |
| --- | --- | --- | --- |
| F1 | **Upload** to Logos Storage → CID | **DONE** | `HttpStorage`/`Publisher::upload` tested; a **live** Codex node on the mini returned CID `zDvZRwzk…`. |
| F2 | **Broadcast** envelope over Logos Delivery | **DONE** (host + dev path) / **PARTIAL** (live Waku) | `HttpDelivery`/`Publisher::broadcast` tested against the documented nwaku REST; a live Waku node wasn't runnable on this Mac (dev path used; works on Linux). |
| F2a | Minimum envelope fields | **DONE** | `MetadataEnvelope`, all required fields, unit-tested. |
| F2b | Canonical, language-agnostic `metadata_hash` | **DONE** | Length-prefixed, domain-separated SHA-256; Rust + C++/QML share a vector. |
| F3 | **On-chain anchoring** of `(cid, metadata_hash)`, permissionless | **DONE** | Program deployed; **real `anchor_one` AND `anchor_batch` round trips confirmed** with `RISC0_DEV_MODE=0` — 12- and 50-CID batches each in one tx, every record read back by CID. Idempotent + permissionless (signer-less IDL → empty witness; no funded signer required). |
| F3a | One PDA per CID, deterministic from the CID | **DONE** | `cid_seed = SHA256("WB-CID-PDA-v1"\|\|cid)`; the program and the off-chain `pda` helper derived the **same** PDA, confirmed on-chain. |
| F3b | Idempotent anchoring | **DONE** | Claim-if-default in the program; verified the live record is stable; mirrored/tested in `MockRegistry`/`FileRegistry`. |
| F4 | **Batch anchor tool** (≥10; 50 target), standalone + permissionless | **DONE** | `BatchAnchorRunner` (default 50) + `anchor_batch(Vec<…>)`; the standalone CLI runs the full publish→accumulate→anchor→query loop. On-chain batch **verified live**: a **12-CID** batch (tx `79790d08…`) and a **50-CID** batch each landed in ONE tx with every record read back by CID (`RISC0_DEV_MODE=0`). |
| F4a | Batch via a SPEL client (not the IDL CLI) | **DONE** (via adapter; IDL CLI blocked upstream) | The SPEL *IDL CLI* still can't encode a `Vec<struct>` arg (issue #1) — so the batch is submitted by our `wb-lez-registry` adapter, a custom client that builds the same risc0 word-serde `Instruction` value the SPEL-generated client would. **Verified live** (12- and 50-CID). |
| F5 | **On-chain registry** queryable by CID | **DONE** | `inspect <PDA> --type RegistryRecord` decoded the stored record (cid + metadata_hash + anchor_timestamp) live; `get_by_cid` + CLI `query` tested file-backed. |
| F6 | **Document-indexing module** (reusable) | **DONE** | `wb-index` — standalone module (Storage/Delivery/Registry clients, `Publisher`, `BatchAnchorRunner`, checkpoints); 21 unit tests. |

## Usability

| # | Criterion | Status | Note |
| --- | --- | --- | --- |
| U1 | **Basecamp GUI** | **PARTIAL** | Complete `ui_qml` module in source (QML + C++ backend, `logos` bridge, module deps). Not built — needs Nix + Qt6 (not on this hardware). |
| U2 | **Module README** | **DONE** | `wb-index`, app, program, and root READMEs + `docs/ARCHITECTURE.md`. |
| U3 | **IDL via SPEL** | **DONE** (IDL) / **PARTIAL** (clients) | IDL generated + committed (lists `anchor_batch` + `anchor_one`); `spel-client-gen` client emission is the remaining step (HANDOFF §4). |

## Reliability

| # | Criterion | Status | Note |
| --- | --- | --- | --- |
| R1 | **Upload retry** (back-off) | **DONE** | `RetryPolicy` exponential back-off in `Publisher::upload`; unit-tested. |
| R2 | **Broadcast/anchor dedup** | **DONE** | CID dedup + registry idempotency; unit-tested. |
| R3 | **Resumable batch** | **DONE** | Atomic `CheckpointStore`; resume tested (incl. cross-process). |

## Performance

| # | Criterion | Status | Note |
| --- | --- | --- | --- |
| P1 | **Cycle benchmarks**, single-CID vs 50-CID | **DONE** | Real on-chain `RISC0_DEV_MODE=0` (M4): single-CID **~3.0 ms**, 50-CID **~48.6 ms** = **0.97 ms/CID** (~3× cheaper per CID), 3 fresh runs each (`docs/benchmarks.md`). LEZ has no "compute unit" and public anchors have no per-tx STARK proof, so we report executor time; isolated RISC0 cycle counts (a nicety) would need an executor `cycle_bench` harness. |

## Supportability

| # | Criterion | Status | Note |
| --- | --- | --- | --- |
| S1 | **Deployed on devnet/testnet** | **DONE** (standalone) | Deployed to a standalone LEZ sequencer; program id recorded (image id above). No public LEZ testnet RPC exists upstream; standalone is the documented path and what LP-0017's demo criteria call for. |
| S2 | **Standalone-sequencer integration tests in CI** | **PARTIAL** | `wb-e2e` integration test gates CI; a sequencer-backed job is best-effort (`continue-on-error`) — the LEZ stack isn't reproducible in plain CI. The real sequencer run is demonstrated on the mini. |
| S3 | **CI green** | **DONE** (gating job) | The gating `workspace` job (fmt + build + test + clippy, Rust 1.94) is green. |
| S4 | **README** | **DONE** | Architecture, LEZ-program rationale, build/run, query-by-CID, criteria, filed issues. |
| S5 | **Reproducible demo, `RISC0_DEV_MODE=0`** | **DONE** (on-chain) / **PARTIAL** (full pipeline) | The anchor→confirm→query demo runs live with `RISC0_DEV_MODE=0` (real proving). The full upload→broadcast→anchor with a live Delivery node uses the dev path here; `scripts/demo.sh` runs it on Linux. |
| S6 | **Recorded video** (narrated, real proof) | **DONE** | Narrated 1920×1080 walkthrough, `RISC0_DEV_MODE=0`: [demo video](https://github.com/jefdiesel/whistleblower-lp0017/releases/download/v0.1.0/whistleblower-lp0017-narrated.mp4) ([release](https://github.com/jefdiesel/whistleblower-lp0017/releases/tag/v0.1.0)). Real Codex CIDs + canonical SHA-256 hashes; shows upload → single + 12/50-CID batch anchor → query-by-CID → benchmarks. |
| S7 | **Public PR** (submission) | **REMAINING (human-only)** | Open the public repo + solution PR to `logos-co/lambda-prize`. |
| S8 | Dual license **MIT OR Apache-2.0** | **DONE** | Dual-licensed root + per crate; SPDX headers. |

---

## Where it stands

- **The hard, prize-critical core is DONE and verified on real hardware:** the
  upload→…→**on-chain anchor (single + 12/50-CID batch) + query-by-CID** path runs
  live with real proving (`RISC0_DEV_MODE=0`), plus real benchmarks, a live Storage
  node, and 30 green tests.
- **PARTIAL (clear finish):** live Waku broadcast (any Linux box), Basecamp app
  build (Nix+Qt6), SPEL-generated IDL clients (`spel-client-gen`).
- **REMAINING:** open the public PR to `logos-co/lambda-prize`; optionally file the
  upstream issues in `docs/ISSUES-TO-FILE.md`. (The narrated **video is published**:
  see the v0.1.0 release.)

Finishing procedure: **[HANDOFF.md](./HANDOFF.md)**. Current detail: **[docs/STATUS.md](./docs/STATUS.md)**.
