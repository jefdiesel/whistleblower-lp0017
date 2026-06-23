<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Whistleblower — Architecture

Whistleblower (Logos prize **LP-0017**) is a censorship-resistant document
upload and indexing tool built on the Logos stack: **Logos Storage** (Codex),
**Logos Delivery** (Waku), and the **Logos Execution Zone** (LEZ). A document is
uploaded to Storage, a metadata envelope is broadcast over Delivery so it is
immediately discoverable, and a `(cid, metadata_hash)` tuple is anchored
on-chain in a permissionless LEZ registry program so the document's existence
and integrity are verifiable and queryable by CID.

This document describes the components, the data-flow, the canonical hashing
spec, the Delivery topic, the registry PDA scheme, idempotency / checkpoint /
resume, and the trust model.

---

## 1. Components and responsibilities

| Component | Crate / path | Responsibility |
| --- | --- | --- |
| Shared types | `crates/wb-types` | The data shapes that cross every boundary: `MetadataEnvelope`, the canonical `metadata_hash`, `RegistryRecord`, `AnchorEntry`, and the Delivery `topic`. Dependency-light so the **same** types compile for host binaries and for the RISC0 zkVM guest (`default-features = false`). |
| Indexing module | `crates/wb-index` | The reusable **upload → broadcast → anchor** pipeline. `HttpStorage` (Codex REST), `HttpDelivery` (Waku REST), the `RegistryClient` trait with `MockRegistry` + `FileRegistry`, the `Publisher`, and the `BatchAnchorRunner` (checkpoint / resume). Usable by any Logos app, not just this one. |
| Batch-anchor CLI | `crates/wb-batch-anchor` | The permissionless command-line tool. Defines the shared `Cli` + generic `run` driver and the `wb-batch-anchor` binary (file-backed registry, for local/dev/CI). Subcommands: `publish`, `run`, `anchor`, `query`, `status`. |
| On-chain registry program | `crates/wb-registry-program` *(excluded workspace)* | The SPEL `#[lez_program] mod whistleblower_registry` — the actual on-chain program: `anchor_batch`, one PDA per CID, stores `borsh(RegistryRecord)`, idempotent. Built with the RISC0 toolchain + SPEL; IDL via `make idl`. |
| On-chain registry adapter | `crates/wb-lez-registry` *(excluded workspace)* | The real on-chain `RegistryClient` (`LezRegistry`) that talks to a LEZ sequencer over JSON-RPC, plus the `wb-batch-anchor-lez` binary (same CLI, real anchoring with proofs). |
| Basecamp app | `app/` | The Qt6/QML Logos Core `ui_qml` module. Uploads via `storage_module`, broadcasts via `delivery_module`, anchors via the generated `whistleblower_registry` module. Built with Nix. |
| Integration tests | `tests/` (`wb-e2e`) | Black-box end-to-end tests wiring the whole pipeline together with in-process fakes (content-addressed in-memory storage, a shared in-memory delivery queue, `MockRegistry`). |

The two `crates/wb-registry-program` and `crates/wb-lez-registry` crates are
**excluded** from the root Cargo workspace: they pin LEZ `v0.1.2` and pull a
riscv32 RISC0 zkVM build graph that must not contaminate the host build. The
generic `RegistryClient` trait in `wb-index` is the seam that lets the rest of
the codebase build and test anywhere.

---

## 2. Data-flow

```
                         (1) upload bytes
  ┌──────────┐    POST /api/storage/v1/data    ┌──────────────────────┐
  │  a file  │ ───────────────────────────────▶│  Logos Storage        │
  └──────────┘                                  │  (Codex, REST :8080)  │
        │                                       └──────────┬───────────┘
        │                                          returns │ CID
        │                                                  ▼
        │                         build MetadataEnvelope { cid, title, …, tags }
        │
        │   (2) broadcast envelope (JSON)
        │   POST /relay/v1/auto/messages/<topic>           ┌──────────────────────┐
        └─────────────────────────────────────────────────▶│  Logos Delivery       │
                topic = /whistleblower/1/documents/json     │  (Waku, REST :8645)   │
                                                            └──────────┬───────────┘
                                                                       │ subscribers
                                                                       │ drain topic
                            (3) accumulate (cid, metadata_hash) tuples ▼
                                              ┌────────────────────────────────────┐
                                              │  wb-batch-anchor[-lez]              │
                                              │  BatchAnchorRunner                 │
                                              │   poll → dedup → batch → checkpoint│
                                              └──────────────────┬─────────────────┘
                                                 (4) anchor_batch │ (entries, ts)
                                                    JSON-RPC :3040│
                                                                 ▼
                                              ┌────────────────────────────────────┐
                                              │  LEZ sequencer + whistleblower_     │
                                              │  registry program (RISC0 zkVM proof)│
                                              │   one PDA per CID:                 │
                                              │   pda = for_public_pda(prog,        │
                                              │     SHA256("WB-CID-PDA-v1"||cid))   │
                                              │   data = borsh(RegistryRecord)     │
                                              └──────────────────┬─────────────────┘
                                                 (5) query by CID│ point-read the PDA
                                                                 ▼
                                                      RegistryRecord { cid,
                                                        metadata_hash, anchor_timestamp }
```

1. **Upload** — `Publisher::upload` POSTs the bytes to Logos Storage and gets a
   CID back. Transient Storage failures are retried with exponential back-off
   (`RetryPolicy`).
2. **Broadcast** — `Publisher::broadcast` builds a `MetadataEnvelope`, serializes
   it to JSON, and publishes it to the Delivery content topic. Re-broadcasts of
   the same CID are de-duplicated.
3. **Accumulate** — the permissionless `BatchAnchorRunner` subscribes to the
   topic, drains messages, validates and de-duplicates by CID, and accumulates
   `(cid, metadata_hash)` tuples.
4. **Anchor** — when a batch is full (or on the flush cadence / on shutdown), the
   runner calls `RegistryClient::anchor_batch`. `FileRegistry` writes a JSON
   file; `LezRegistry` submits an on-chain `anchor_batch` transaction.
5. **Query** — `RegistryClient::get_by_cid` derives the PDA from the CID and
   point-reads the on-chain account (no scan), returning the `RegistryRecord`.

---

## 3. The `MetadataEnvelope`

The envelope is the JSON payload broadcast over Delivery (it is also what the app
assembles). Its minimum field set is mandated by LP-0017. Defined in
`crates/wb-types/src/envelope.rs`:

| Field | Type | Notes |
| --- | --- | --- |
| `cid` | `String` | Logos Storage content identifier. |
| `title` | `String` | Human title. |
| `description` | `String` | Free-form; may be empty (serde default). |
| `content_type` | `String` | MIME type, e.g. `application/pdf`. |
| `size_bytes` | `u64` | Uploaded byte length. |
| `timestamp` | `u64` | Publication time, Unix **milliseconds** (UTC). |
| `tags` | `Vec<String>` | Optional; order- and duplicate-insensitive for hashing. |
| `schema_version` | `u16` | Defaults to `SCHEMA_VERSION = 1`. |

`validate()` rejects an empty `cid`, an empty `content_type`, or a
`schema_version` other than the current one.

---

## 4. Canonical `metadata_hash` specification

The registry stores `metadata_hash` rather than the full envelope, and the
batch-anchor tool transports it. To keep that hash reproducible across languages
— the Rust clients **and** the C++/QML Basecamp app — the encoding is explicit
and self-describing rather than relying on any serializer's field/key ordering.

This mirrors `crates/wb-types/src/hash.rs` (and the byte-for-byte C++ port in
`app/src/WhistleblowerBackend.cpp`):

```text
metadata_hash = SHA256(
     LP("WB-META-v1")          // domain separator (encodes schema version)
  || LP(cid)
  || LP(title)
  || LP(description)
  || LP(content_type)
  || u64_le(size_bytes)
  || u64_le(timestamp)
  || u32_le(N)                 // N = number of sorted, de-duplicated tags
  || LP(tag_0) || .. || LP(tag_{N-1})
)

where LP(s)  = u32_le(byte_len(s)) || utf8_bytes(s)
      u64_le / u32_le = little-endian integer encoding
```

Rules that make the hash canonical:

- **Domain separation.** The leading `LP("WB-META-v1")` binds the hash to schema
  version 1. `schema_version` is **not** hashed directly — it is bound through
  this domain string. A schema bump requires a new `META_HASH_DOMAIN` (and a new
  `SCHEMA_VERSION`) so old and new hashes can never collide.
- **Length-prefixed strings.** Every string is `u32_le` length-prefixed before
  its UTF-8 bytes, so no field boundary is ambiguous.
- **Sorted, de-duplicated tags.** Tags are sorted by UTF-8 byte order and
  de-duplicated before hashing. Two envelopes differing only in tag order or
  repeated tags hash identically. (Note: the C++/QML side must sort by UTF-8
  byte order, **not** locale-aware `QString` ordering, to match Rust's
  `sort_unstable()` on `&str`.)

`canonical_metadata_bytes(env)` exposes the exact preimage so other-language
implementations can be checked against a known vector. A shared test vector is
documented at the top of `app/src/WhistleblowerBackend.cpp` and exercised by the
Rust tests in `hash.rs`:

```text
cid          = "zDvSampleCidAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
title        = "Leaked memo"
description  = "Internal memo describing X"
content_type = "application/pdf"
size_bytes   = 12345
timestamp    = 1700000000000
tags         = ["leak","memo"]
```

The on-chain registry stores this hash and the batch-anchor tool recomputes it
from the broadcast envelope, so any divergence between the Rust and C++
implementations makes a document un-anchorable / unverifiable.

---

## 5. The Delivery content topic

Waku content topics follow `/<application>/<version>/<topic-name>/<encoding>`.
Whistleblower broadcasts document envelopes (JSON) to a single well-known topic
so **any** party — including the permissionless batch-anchor tool — can subscribe
and discover newly published documents (`crates/wb-types/src/topic.rs`):

```
/whistleblower/1/documents/json
```

- `whistleblower` — application segment
- `1` — application version
- `documents` — topic-name segment (document-envelope broadcasts)
- `json` — encoding (JSON-encoded `MetadataEnvelope`)

The same string is hard-coded in the app (`app/qml/Main.qml`) and is the default
for the CLI's `--topic` / `WB_TOPIC`. `HttpDelivery` percent-encodes the topic in
the relay REST URL paths.

---

## 6. Registry PDA scheme (`cid_seed`)

The registry follows a Solana-like model: **one account per document**, each in
its own program-derived account (PDA) derived deterministically from the CID. The
derivation is the single source of truth in
`crates/wb-registry-program/wb_registry_core/src/lib.rs`, shared by the on-chain
guest and the off-chain host so they always agree:

```rust
pub const CID_PDA_DOMAIN: &[u8] = b"WB-CID-PDA-v1";

pub fn cid_seed(cid: &str) -> [u8; 32] {
    // SHA256("WB-CID-PDA-v1" || cid)
}
```

CIDs (e.g. base32 CIDv1, ~59 chars) exceed the 32-byte PDA seed limit, so the CID
is hashed into a 32-byte seed.

- **On-chain** (the guest), the account is validated against
  `spel_framework::pda::compute_pda(&ctx.self_program_id, &[&cid_seed(cid)])`.
- **Off-chain** (the host adapter / query path), the account id is reproduced as
  `AccountId::for_public_pda(&program_id, &PdaSeed::new(cid_seed(cid)))`.

Each PDA's account data is `borsh(RegistryRecord { cid, metadata_hash,
anchor_timestamp })`. Because the address is a pure function of the CID, a lookup
is a single **O(1) point-read** — no on-chain index or enumeration — which is
what makes the registry "queryable by CID".

### `anchor_batch` instruction

```text
anchor_batch(
    ctx,
    entries: Vec<AnchorArg { cid, metadata_hash }>,
    anchor_timestamp: u64,
    #[account(mut)] records: Vec<AccountWithMetadata>,   // records[i] is the PDA for entries[i].cid
)
```

The program checks `records.len() == entries.len()`, validates each provided
account is the canonical PDA for its CID, and writes
`borsh(RegistryRecord)` into each previously-unwritten account. It returns
`SpelOutput::execute(post_states, vec![])`.

> **No on-chain events.** The LEZ `v0.1.2` / SPEL stack has no event mechanism
> (the LP-0012 `emit_event` API does not exist at this version), so the program
> emits none; discovery happens off-chain via the Delivery topic and queries are
> point-reads of the PDA.

---

## 7. Idempotency, checkpoint, and resume

**Idempotency (on-chain).** `anchor_batch` only initializes an *unowned (default)*
account; an already-registered account is passed through unchanged via
`AccountPostState::new_claimed_if_default`. So re-anchoring a known CID — or two
anchorers racing on the same CID — is a safe no-op and never fails. This is what
makes the permissionless batch loop crash-safe: re-submitting a batch after a
network drop cannot corrupt or double-write state.

**Idempotency (off-chain receipts).** The `MockRegistry` / `FileRegistry`
compute a precise split of newly-anchored vs already-present CIDs. `LezRegistry`
does **not** pay the round-trip cost to compute that split on the hot path: the
sequencer's submit response doesn't itself say which CIDs were newly written, so
it reports every submitted CID under `AnchorReceipt.anchored` and leaves
`already_present` empty. Downstream logic treats both buckets as "anchored".

**Dedup.** `CidDedup` (a hash set) is used both publisher-side (suppress
re-broadcasts of the same CID) and subscriber-side (skip already-seen / anchored
CIDs).

**Checkpoint / resume.** `BatchAnchorRunner` persists a `Checkpoint` via
`CheckpointStore` (atomic write-tmp + rename to a JSON file, default
`.wb/checkpoint.json`):

| Field | Meaning |
| --- | --- |
| `anchored_cids` | Every CID known to be on-chain. |
| `last_delivery_timestamp_ns` | Highest Delivery message timestamp seen (informational). |
| `last_batch_tx` | Tx hash of the most recent committed batch. |
| `batches_committed` | Lifetime batch count. |

On `init()`, the checkpoint is loaded and its `anchored_cids` seed the dedup set,
so already-anchored CIDs are never re-processed across restarts. `anchored_cids`
grows on each flush, so progress is never lost. The runner flushes when the
pending batch reaches `batch_size` (default 50; the on-chain path expects ≥ 10),
on the `flush_interval` cadence, and once more on shutdown.

---

## 8. Trust model

- **Anchoring is permissionless.** Anyone can run a `wb-batch-anchor-lez`
  instance against the public Delivery topic and the registry program. The signer
  on a transaction only authorizes/pays for that transaction; it grants no
  special registry privilege. Idempotency means many independent anchorers
  converge on the same state without conflict. This permissionlessness is the
  reason the registry is a **LEZ program** rather than a zone-SDK construction
  (the zone-SDK path currently needs a single designated actor to perform
  consensus inscription — decentralised sequencers for zones aren't shipped —
  which would break this trust model; see the README for the full justification).
- **The Delivery topic is permissionless.** `/whistleblower/1/documents/json` is
  open: anyone may publish envelopes and anyone may subscribe and discover
  documents. There is no gatekeeper between upload and discoverability.
- **`anchor_timestamp` is submitter-attested.** LEZ `v0.1.2` exposes no on-chain
  clock the guest can read deterministically, so the `anchor_timestamp` stored in
  each `RegistryRecord` is supplied by the submitter and applies to every entry in
  the batch. It should be read as "the anchorer asserts it anchored this CID at
  approximately time *t*", **not** as a trustless on-chain timestamp.
- **What the registry *does* guarantee.** Given a CID, the on-chain
  `metadata_hash` is the canonical hash of the envelope that was anchored, and the
  PDA's existence proves the CID was anchored. Because the hash is
  content-addressed over the envelope and the CID is content-addressed over the
  bytes, a verifier can independently re-derive both and detect tampering. The
  document bytes themselves live in Logos Storage (Codex), addressed by the CID.

---

## See also

- `README.md` — build/run instructions, success-criteria mapping, issues filed.
- `HANDOFF.md` — finishing the on-chain build on a provisioned machine.
- `docs/benchmarks.md` — RISC0 cycles (not "CU") and how to measure them.
- `crates/wb-index/README.md`, `crates/wb-lez-registry/README.md`,
  `app/README.md` — per-component detail.
