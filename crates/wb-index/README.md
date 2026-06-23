# wb-index — Whistleblower document-indexing module

A self-contained Rust library implementing the censorship-resistant
**upload → broadcast → anchor** pipeline on the Logos stack. It is extracted from
the Whistleblower app so **any Logos application can reuse it** without depending
on the Whistleblower Basecamp app.

```
            ┌─────────────┐   bytes    ┌──────────────────┐
  file ───▶ │  Publisher  │ ─────────▶ │  Logos Storage   │ ──▶ CID
            │  (upload +  │            │  (HttpStorage)   │
            │  broadcast) │   envelope ┌──────────────────┐
            └─────────────┘ ─────────▶ │  Logos Delivery  │ ──▶ subscribers
                                        │  (HttpDelivery)  │
                                        └──────────────────┘
            ┌────────────────────┐  (cid, metadata_hash)   ┌──────────────┐
  topic ──▶ │ BatchAnchorRunner  │ ──────────────────────▶ │ RegistryClient│ ─▶ on-chain
            │ (accumulate+batch) │   single batch tx        │ (LEZ / Mock) │
            └────────────────────┘                          └──────────────┘
```

## Design

Three traits form the seam between platform-agnostic logic and the Logos APIs:

| Trait | Responsibility | Provided implementations |
|-------|----------------|--------------------------|
| [`StorageClient`] | store bytes → CID, fetch by CID | `HttpStorage` (Codex REST) |
| [`DeliveryClient`] | publish/subscribe/poll a content topic | `HttpDelivery` (Waku REST) |
| [`RegistryClient`] | anchor `(cid, metadata_hash)` batches, query by CID | `MockRegistry` (in-mem); `LezRegistry` lives in the `wb-lez-registry` crate |

The on-chain adapter is deliberately **not** in this crate: it pulls the heavy
RISC0/LEZ build dependencies, so it ships separately as `wb-lez-registry`. This
crate stays light and builds anywhere.

Two high-level drivers sit on top:

- **`Publisher`** — the GUI/publish flow: `upload` (with exponential-backoff retry
  on transient Storage failures) then `broadcast` (deduplicated by CID).
- **`BatchAnchorRunner`** — the permissionless batch-anchor loop: subscribe →
  accumulate → anchor in batches → **checkpoint** → resume after interruption,
  with idempotent re-submission.

## Quick start — publish a document

```rust
use wb_index::{Publisher, PublishMeta, HttpStorage, HttpDelivery};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let publisher = Publisher::new(
        HttpStorage::new("http://localhost:8080/api/storage/v1"),
        HttpDelivery::new("http://127.0.0.1:8645"),
    );

    let bytes = std::fs::read("leak.pdf")?;
    let outcome = publisher.publish(&bytes, PublishMeta {
        title: "Internal memo".into(),
        description: "Describes X".into(),
        content_type: None,                 // inferred from filename
        filename: Some("leak.pdf".into()),
        tags: vec!["leak".into()],
    }).await?;

    println!("CID {} (broadcast: {})", outcome.cid, outcome.broadcast);
    Ok(())
}
```

`outcome.envelope` is the [`wb_types::MetadataEnvelope`] that was broadcast; its
canonical [`metadata_hash`](../wb-types) is what gets anchored on-chain.

## Quick start — run the batch anchorer

```rust
use std::time::Duration;
use wb_index::{BatchAnchorRunner, RunnerConfig, CheckpointStore, HttpDelivery, MockRegistry};

let runner = BatchAnchorRunner::new(
    HttpDelivery::new("http://127.0.0.1:8645"),
    MockRegistry::new(),                          // swap for LezRegistry in production
    CheckpointStore::new(".wb/checkpoint.json"),  // enables resume
    RunnerConfig { batch_size: 50, ..Default::default() },
);
// runner.run(shutdown_future).await?;            // loops until shutdown
```

To anchor on-chain for real, depend on `wb-lez-registry` and pass a
`LezRegistry` instead of `MockRegistry` — every other line is identical (see
that crate's README and `../../HANDOFF.md`).

## Integrating into another Logos app

1. Add the dependency:
   ```toml
   wb-index = { git = "https://github.com/your-org/whistleblower", package = "wb-index" }
   ```
2. Pick your transport implementations (`HttpStorage`/`HttpDelivery`, or your own
   types implementing the traits — e.g. an FFI bridge to the C++ `storage_module`
   / `delivery_module` inside a Basecamp app).
3. Use `Publisher` to publish, and `BatchAnchorRunner` (or call `RegistryClient`
   directly) to anchor. For on-chain anchoring add `wb-lez-registry`.

Everything is generic over the three traits, so tests and dev runs use
`MockRegistry` and the loop logic is exercised without any running node.

## Reliability guarantees (LP-0017)

- **Upload retry**: `Publisher::upload` retries transient Storage failures
  (timeouts, connect errors, HTTP 5xx/429) with exponential backoff and returns
  `IndexError::UploadRetriesExhausted { attempts, source }` after exhaustion.
- **Broadcast dedup**: `Publisher::broadcast` skips a CID already broadcast, so
  subscribers never see duplicate envelopes for the same CID.
- **Resumable anchoring**: `BatchAnchorRunner` persists anchored CIDs to a
  `CheckpointStore`; after a restart it re-seeds its dedup set and never
  re-processes registered CIDs. Idempotent anchoring makes a re-submitted batch
  (e.g. after a mid-transaction network drop) a safe no-op.

## Testing

```bash
cargo test -p wb-index      # 20 unit tests, no running nodes required
```

[`StorageClient`]: src/storage.rs
[`DeliveryClient`]: src/delivery.rs
[`RegistryClient`]: src/registry.rs
[`Publisher`]: src/publisher.rs
[`BatchAnchorRunner`]: src/runner.rs
[`wb_types::MetadataEnvelope`]: ../wb-types/src/envelope.rs
