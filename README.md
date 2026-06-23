<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Whistleblower

**Logos prize LP-0017 — a censorship-resistant document upload and indexing
tool on the Logos stack.**

Whistleblower lets anyone publish a document so that it is durable, immediately
discoverable, and verifiable — without a trusted intermediary. A file's bytes are
uploaded to **Logos Storage** (Codex), yielding a content identifier (CID); a JSON
metadata envelope describing it is broadcast over **Logos Delivery** (Waku) to a
well-known, permissionless topic so any subscriber sees it instantly; and a
`(cid, metadata_hash)` tuple is anchored **on-chain** in a permissionless
**Logos Execution Zone** (LEZ) registry program — one program-derived account
(PDA) per CID — so the document's existence and the integrity of its metadata are
provable and **queryable by CID**. A Qt6/QML Logos Basecamp app drives the
upload/broadcast/anchor flow, and a standalone, permissionless batch-anchor CLI
indexes the topic and commits CIDs on-chain in idempotent, resumable batches.

---

## What works today

The **Rust core builds and is tested on any machine** (no RISC0, Nix, or live
nodes required), pinned to Rust **1.94**:

```sh
cargo test --workspace      # 27 unit tests (wb-types: 6, wb-index: 21) + the wb-e2e pipeline test
```

- **`wb-types`** — the shared, dependency-light types: `MetadataEnvelope`, the
  canonical `metadata_hash`, `RegistryRecord`, `AnchorEntry`, the Delivery topic.
  (6 unit tests.)
- **`wb-index`** — the reusable **upload → broadcast → anchor** module:
  `HttpStorage` (Codex REST), `HttpDelivery` (Waku REST), the `RegistryClient`
  trait with `MockRegistry` and `FileRegistry`, the `Publisher`, and the
  `BatchAnchorRunner` with checkpoint/resume. (21 unit tests.)
- **`wb-batch-anchor`** — the permissionless CLI (`publish` / `run` / `anchor` /
  `query` / `status`); the local binary uses a file-backed registry so the full
  publish→query→checkpoint flow works with no sequencer.
- **`tests/` (`wb-e2e`)** — a black-box end-to-end test that wires the entire
  pipeline together with in-process fakes (content-addressed in-memory storage, a
  shared in-memory delivery queue, `MockRegistry`).

A **no-node developer demo** runs all of this locally:

```sh
bash scripts/dev-demo.sh
```

The on-chain pieces are **complete in source**, and the SPEL **IDL is generated
and committed** (`crates/wb-registry-program/whistleblower_registry-idl.json`).
What is **not yet done** — and is honestly tracked as remaining — is everything
that needs a provisioned machine with the RISC0/SPEL/LEZ toolchain (and Nix/Qt
for the app): compiling the zkVM guest to a real `riscv32im-risc0-zkvm-elf`
binary, deploying it to a standalone LEZ sequencer, running the real-proof
end-to-end demo with `RISC0_DEV_MODE=0`, filling in the cycle benchmarks, and
building the Basecamp app. RISC0/Nix/Qt are **not installed on the authoring
machine**, so none of those steps have been executed here yet. The full,
ordered procedure to finish them on a provisioned host (target: a Mac mini M4)
is **[HANDOFF.md](./HANDOFF.md)**, and the per-criterion status is in
**[SUBMISSION.md](./SUBMISSION.md)**.

> **Status, stated plainly:** the program is **not yet deployed** and **no real
> proof has been generated**; the on-chain `LezRegistry` adapter
> (`crates/wb-lez-registry`) is unverified scaffolding against LEZ `v0.1.2` (it
> is full of `TODO(verify against LEZ v0.1.2)` markers and `parse_signer_key`
> fails loudly on purpose), so the on-chain `wb-batch-anchor-lez` binary cannot
> submit a transaction until those are wired on a build host. Do not read this
> README as claiming a live deployment.

---

## Repository layout

| Path | What it is | Builds in root workspace? |
| --- | --- | --- |
| `crates/wb-types/` | Shared types: envelope, canonical `metadata_hash`, `RegistryRecord`. | ✅ yes |
| `crates/wb-index/` | Reusable upload→broadcast→anchor module (Storage/Delivery/Registry clients, Publisher, BatchAnchorRunner). | ✅ yes |
| `crates/wb-batch-anchor/` | Permissionless CLI + the file-backed `wb-batch-anchor` binary. | ✅ yes |
| `tests/` (`wb-e2e`) | End-to-end integration tests with in-process fakes. | ✅ yes |
| `crates/wb-registry-program/` | The on-chain SPEL `#[lez_program] whistleblower_registry` (RISC0 zkVM guest) + IDL. | ❌ **excluded** (RISC0/SPEL) |
| `crates/wb-lez-registry/` | The real on-chain `RegistryClient` (`LezRegistry`) + the `wb-batch-anchor-lez` binary. | ❌ **excluded** (LEZ git deps) |
| `app/` | The Qt6/QML Logos Basecamp `ui_qml` module (built with Nix). | n/a (Nix) |
| `scripts/` | Demo and infra scripts (see below). | n/a |
| `docs/` | `ARCHITECTURE.md`, `benchmarks.md`. | n/a |
| `README.md`, `HANDOFF.md` | This file and the provisioned-machine handoff. | n/a |

The two `crates/wb-registry-program` and `crates/wb-lez-registry` crates are
**`exclude`d** from the root Cargo workspace on purpose: they pin LEZ `v0.1.2` and
pull a riscv32 RISC0 zkVM build graph that must not contaminate the host build.
The generic `RegistryClient` trait in `wb-index` is the seam that lets everything
else build and test anywhere.

### Scripts

> All scripts begin with `#!/usr/bin/env bash`. Make them executable once with
> `chmod +x scripts/*.sh`, or run them as `bash scripts/<name>.sh`.

| Script | Runs where | Purpose |
| --- | --- | --- |
| `scripts/dev-demo.sh` | **here** (no nodes) | `cargo test`, build the CLI, show `--help` / `status` / `query` against a file registry. The honest no-infra demo. |
| `scripts/run-nodes.sh` | provisioned | Start a local Codex storage node (REST :8080) and a Waku delivery node (REST :8645). Best-effort docker/binary commands. |
| `scripts/run-sequencer.sh` | provisioned | Run the LEZ standalone sequencer (JSON-RPC :3040) with `RISC0_DEV_MODE=0`. |
| `scripts/demo.sh` | provisioned | The **real** end-to-end: `publish` → on-chain `anchor` → `query`. |
| `scripts/sample-doc.txt` | — | Sample document used by `demo.sh`. |

---

## Architecture summary

```
                         (1) upload bytes
  ┌──────────┐    POST /api/storage/v1/data    ┌──────────────────────┐
  │  a file  │ ───────────────────────────────▶│  Logos Storage        │
  └──────────┘                                  │  (Codex, REST :8080)  │
        │                                       └──────────┬───────────┘
        │                                          returns │ CID
        │                         build MetadataEnvelope { cid, title, …, tags }
        │   (2) broadcast envelope (JSON)
        │   POST /relay/v1/auto/messages/<topic>           ┌──────────────────────┐
        └─────────────────────────────────────────────────▶│  Logos Delivery       │
                topic = /whistleblower/1/documents/json     │  (Waku, REST :8645)   │
                                                            └──────────┬───────────┘
                            (3) accumulate (cid, metadata_hash) tuples ▼
                                              ┌────────────────────────────────────┐
                                              │  wb-batch-anchor[-lez]              │
                                              │  poll → dedup → batch → checkpoint │
                                              └──────────────────┬─────────────────┘
                                                 (4) anchor_batch │  JSON-RPC :3040
                                                                 ▼
                                              ┌────────────────────────────────────┐
                                              │  LEZ whistleblower_registry program │
                                              │  one PDA per CID (RISC0 zkVM proof) │
                                              │   data = borsh(RegistryRecord)     │
                                              └──────────────────┬─────────────────┘
                                                 (5) query by CID│  point-read the PDA
                                                                 ▼
                                                      RegistryRecord { cid,
                                                        metadata_hash, anchor_timestamp }
```

`file → Storage[CID] → Delivery[envelope] → BatchAnchor → LEZ registry[PDA per CID]`.

The canonical `metadata_hash` (the thing the registry stores) is a length-
prefixed, domain-separated SHA-256 over the envelope fields, with tags sorted and
de-duplicated, so it is byte-reproducible across the Rust clients and the C++/QML
app. The PDA for a CID is
`for_public_pda(program_id, SHA256("WB-CID-PDA-v1" || cid))`, so "query by CID" is
a single O(1) point-read. Full detail in
**[docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md)**.

### Why a LEZ program, not the zone SDK

The tool must be **permissionless**: anyone should be able to anchor a CID, and
many independent anchorers should converge on the same registry state without a
designated operator.

- The **zone-SDK** path needs a **single designated actor** to perform consensus
  inscription — decentralised sequencers for zones are not shipped at this LEZ
  version. Routing every anchor through one actor reintroduces exactly the trusted
  intermediary the project exists to remove, so it breaks the trust model.
- The **LEZ-program** path lets anyone submit a transaction that invokes
  `anchor_batch`; the program is idempotent, so concurrent/duplicate anchors are
  safe no-ops. Anchoring is genuinely permissionless.
- LP-0017's **SPEL-IDL requirement** applies to the LEZ-program path (the program
  exposes a SPEL IDL from which typed clients and the app's module are generated).

For these reasons the registry is implemented as a SPEL `#[lez_program]`. The
trust-model consequences (permissionless topic + registry, submitter-attested
`anchor_timestamp`) are detailed in `docs/ARCHITECTURE.md`.

---

## Build & run

### (a) Build + test the workspace

```sh
# Rust 1.94 (rust-toolchain.toml pins it).
cargo build --workspace
cargo test --workspace      # 27 unit tests + the wb-e2e pipeline test
```

### (b) Local dev demo (no external nodes)

```sh
chmod +x scripts/*.sh          # once
scripts/dev-demo.sh            # or: bash scripts/dev-demo.sh
```

This runs the tests, builds `wb-batch-anchor`, prints its `--help`, seeds a
**file-backed** registry and runs `query`/`status` against it, and prints clear
"this is the no-node dev demo" messaging — being explicit that `publish` and
`run`/`anchor` against the network need live nodes.

### (c) The Basecamp app (Nix)

Requires **Nix** (flakes) and **Qt6** — it cannot be built without them. From the
provisioned machine:

```sh
cd app
nix build            # build the ui_qml module
nix build .#lgx      # package the .lgx for Logos Basecamp -> ./result
nix develop          # optional: a cmake/ninja/qt6 dev shell
```

The app declares three Logos Core module dependencies (`storage_module`,
`delivery_module`, and the **generated** `whistleblower_registry` module). The
generated module is produced from the registry program's IDL — see step (d) and
`app/README.md`. The `flake.nix` is based on the `logos-module-builder`
`tutorial-v1` template and may need small alignment to the installed builder API
(see comments in `app/flake.nix`).

### (d) The registry program (RISC0 + SPEL build, deploy, IDL)

The on-chain program lives in `crates/wb-registry-program` (its own workspace; the
zkVM guest is a *further* detached workspace under `methods/guest`). It needs the
RISC0 toolchain and, for the guest, the LEZ issue-#468 `ring` patch — full
sequence in **[HANDOFF.md](./HANDOFF.md)**. Its `Makefile` targets:

```sh
cd crates/wb-registry-program

make build        # reproducible guest build (Docker): cargo risczero build …
make build-local  # no-Docker guest build with the installed RISC0 toolchain
make idl          # generate the SPEL IDL JSON (whistleblower_registry-idl.json)
make deploy       # wallet deploy-program <bin> — prints the program id (= RISC0 image id)
make ffi-gen      # spel-client-gen … --target rust+ffi    (client for the CLI adapter)
make ui-gen       # spel-client-gen … --target logos-module (the app's generated module)
```

### (e) The batch-anchor CLI (including on-chain `wb-batch-anchor-lez`)

Shared subcommands (both binaries):

```sh
# Local/dev (file-backed registry; no sequencer):
wb-batch-anchor publish <file> --title "…" --description "…" --tags a,b,c
wb-batch-anchor run    --batch-size 50      # daemon: subscribe, accumulate, anchor in batches
wb-batch-anchor anchor                      # one-shot: drain a few rounds, anchor, exit
wb-batch-anchor query  <cid>                # query the registry by CID
wb-batch-anchor status                      # checkpoint stats

# Real on-chain (provisioned machine), same subcommands:
export WB_SEQUENCER_URL=http://localhost:3040
export WB_PROGRAM_ID=<deployed program id>   # = RISC0 image id from `make deploy`
export WB_SIGNER_KEY=<hex signing key>
cd crates/wb-lez-registry && cargo build --release
wb-batch-anchor-lez run --batch-size 50
wb-batch-anchor-lez anchor
wb-batch-anchor-lez query <cid>
```

Common configuration (flags / env, defaults shown):

| Flag | Env | Default |
| --- | --- | --- |
| `--delivery-url` | `WB_DELIVERY_URL` | `http://127.0.0.1:8645` |
| `--storage-url` | `WB_STORAGE_URL` | `http://localhost:8080/api/storage/v1` |
| `--topic` | `WB_TOPIC` | `/whistleblower/1/documents/json` |
| `--checkpoint` | `WB_CHECKPOINT` | `.wb/checkpoint.json` |
| `--registry-file` | `WB_REGISTRY_FILE` | `.wb/registry.json` (file-backed binary only) |

The full real end-to-end demo (nodes + sequencer + deployed program) is
`scripts/demo.sh` (forces `RISC0_DEV_MODE=0`).

### (f) Querying the registry by CID

```sh
wb-batch-anchor      query <cid>           # file-backed (local/dev)
wb-batch-anchor      query <cid> --json
wb-batch-anchor-lez  query <cid>           # on-chain
```

A query derives the PDA from the CID (`SHA256("WB-CID-PDA-v1" || cid)`), reads the
account, and decodes the `RegistryRecord { cid, metadata_hash, anchor_timestamp }`.
A missing CID prints "not found" and exits with code `2`.

---

## Deployment status & addresses

> **Not deployed yet.** The `whistleblower_registry` program has **not** been
> built to a zkVM binary or deployed to any sequencer from this repository, and
> no real proof has been generated. The table below is therefore unfilled on
> purpose — fill it in only **after** an actual `make deploy` on a provisioned
> machine (see **[HANDOFF.md](./HANDOFF.md)** §3–§6). The defaults shown are the
> endpoints the tooling *targets*, not live services.

| Item | Value |
| --- | --- |
| Deployed? | **No — pending the provisioned-machine build/deploy (HANDOFF.md).** |
| `whistleblower_registry` program id (= RISC0 image id) | _not deployed — from `make deploy`_ |
| LEZ sequencer JSON-RPC endpoint | _none running — default target `http://localhost:3040`_ |
| Logos Storage (Codex) REST endpoint | _none running — default target `http://localhost:8080/api/storage/v1`_ |
| Logos Delivery (Waku) REST endpoint | _none running — default target `http://127.0.0.1:8645`_ |
| Delivery content topic | `/whistleblower/1/documents/json` (fixed) |

---

## LP-0017 success criteria

| Criterion | Where satisfied | Status |
| --- | --- | --- |
| Upload a document to Logos Storage and obtain a CID | `wb-index` `HttpStorage` + `Publisher::upload`; app `storage_module.uploadUrl` | ✅ implemented & unit-tested; live upload needs a Codex node |
| Retry uploads on transient Storage failures (back-off) | `wb-index` `RetryPolicy` + `Publisher::upload` | ✅ implemented & tested |
| Broadcast a metadata envelope over Logos Delivery | `wb-index` `HttpDelivery` + `Publisher::broadcast`; app `delivery_module.send`; topic `/whistleblower/1/documents/json` | ✅ implemented & tested; live broadcast needs a Waku node |
| Minimum envelope fields (cid, title, description, content_type, size_bytes, timestamp, tags) | `wb-types` `MetadataEnvelope` | ✅ implemented & tested |
| Canonical, language-agnostic metadata hash | `wb-types` `metadata_hash` (Rust) + `WhistleblowerBackend` (C++/QML), shared test vector | ✅ implemented & tested both sides |
| Anchor `(cid, metadata_hash)` on-chain, permissionlessly | `wb-registry-program` `anchor_batch` (LEZ program); `wb-lez-registry` `LezRegistry`; `wb-batch-anchor-lez` | ⚙️ source complete; build/run on provisioned machine (HANDOFF.md) |
| One account (PDA) per CID, deterministic from the CID | `wb_registry_core::cid_seed` + the program; `LezRegistry::pda_for_cid` | ✅ derivation implemented & tested (`cid_seed`) |
| Idempotent anchoring (safe re-anchor / concurrent anchorers) | program `new_claimed_if_default`; `Mock`/`FileRegistry`; `BatchAnchorRunner` dedup | ✅ implemented & tested |
| Batch anchoring (≥ 10; 50 is the target) | `BatchAnchorRunner` (default `batch_size = 50`); `anchor_batch(Vec<…>)` | ✅ implemented & tested |
| Crash-safe resume across restarts | `Checkpoint` + `CheckpointStore` | ✅ implemented & tested |
| Queryable by CID | `RegistryClient::get_by_cid`; `LezRegistry` PDA point-read; CLI `query` | ✅ implemented & tested (file-backed); on-chain via HANDOFF.md |
| SPEL IDL for the on-chain program | `crates/wb-registry-program` `make idl`; clients via `spel-client-gen` | ⚙️ tooling wired; run on provisioned machine |
| Basecamp app (upload + broadcast + anchor) | `app/` (Qt6/QML `ui_qml`) | ⚙️ source complete; build with Nix on provisioned machine |
| Real proofs in the demo (`RISC0_DEV_MODE=0`), narrated video | `scripts/demo.sh` (+ run-nodes/run-sequencer); `docs/benchmarks.md` | ⚙️ scripted but **not yet run**; video is human-only REMAINING |
| Cost characterization (cycles, not "CU") | `docs/benchmarks.md` (single-CID vs 50-CID) | ⚙️ method + template + reference numbers only; **real numbers REMAINING** (no proof has been run) |

Legend: ✅ done and exercised here · ⚙️ source/tooling complete but **not yet
executed** — requires a provisioned machine (see **[HANDOFF.md](./HANDOFF.md)**).
A ⚙️ means the code/scripts exist, **not** that the on-chain step has happened.
For a full, honest DONE / PARTIAL / REMAINING breakdown of every LP-0017
criterion, see **[SUBMISSION.md](./SUBMISSION.md)**.

---

## Issues filed with Logos tech

While building this we hit (and worked around) several issues in the Logos
toolchain; each is worth filing/tracking upstream:

- **LEZ #468 — `risc0`/`ring` riscv32 cross-compile break.** LEZ enables
  `risc0-zkvm` default features, which pull `ring`, which cannot cross-compile to
  `riscv32im-risc0-zkvm-elf`. Building the zkVM guest requires forking LEZ and
  patching `risc0-zkvm` to `default-features = false` (see HANDOFF.md and the
  `[patch]` block in `crates/wb-registry-program/methods/guest/Cargo.toml`).
- **Storage REST base-path split.** Current Codex nodes serve the API under
  `/api/storage/v1`, while the published JS/Python SDKs target `/api/codex/v1`.
  Clients must be pointed at whichever the node actually serves (we default to
  `/api/storage/v1` and document the split).
- **No documented public LEZ testnet RPC endpoint.** We could not find a
  documented public LEZ testnet sequencer RPC, so the demo runs a **standalone
  sequencer** locally (JSON-RPC :3040).
- **SPEL README staleness.** The SPEL docs reference APIs that don't exist at the
  pinned version: `SpelOutput::states_only` does not exist (the real API is
  `SpelOutput::execute`, which the program uses), and `spel_cli::run()` should be
  `spel::run()`.

---

## License

Dual-licensed under either of:

- **MIT** — see [`LICENSE-MIT`](./LICENSE-MIT)
- **Apache License, Version 2.0** — see [`LICENSE-APACHE`](./LICENSE-APACHE)

at your option. Unless you explicitly state otherwise, any contribution
intentionally submitted for inclusion in this work shall be dual-licensed as
above, without any additional terms or conditions.
