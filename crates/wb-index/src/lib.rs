//! # wb-index — the Whistleblower document-indexing module
//!
//! A self-contained library that implements the censorship-resistant
//! **upload → broadcast → anchor** pipeline once, so any Logos application can
//! reuse it without depending on the Whistleblower Basecamp app.
//!
//! ## The three layers (traits)
//!
//! * [`StorageClient`] — store bytes on Logos Storage, get a CID back.
//! * [`DeliveryClient`] — publish/subscribe document envelopes over Logos Delivery.
//! * [`RegistryClient`] — anchor `(cid, metadata_hash)` tuples on-chain and query by CID.
//!
//! Concrete, ready-to-use implementations are provided for the first two against
//! the real REST APIs ([`HttpStorage`] for Codex-style Storage, [`HttpDelivery`]
//! for Waku-style Delivery). For the registry, [`MockRegistry`] is an in-memory
//! implementation used by tests and local/dev demos; the production on-chain
//! adapter (`LezRegistry`) lives in the separate `wb-lez-registry` crate so this
//! crate stays free of the heavy RISC0/LEZ build dependencies.
//!
//! ## High-level entry points
//!
//! * [`Publisher`] — the publish flow used by the GUI: `upload` (with retry) then
//!   `broadcast` (deduplicated by CID).
//! * [`BatchAnchorRunner`] — the permissionless batch-anchor loop used by the CLI:
//!   subscribe → accumulate → anchor in batches → checkpoint → resume.
//!
//! See `README.md` for an integration walkthrough.
//!
//! The traits use `async fn`; the crate is consumed via static dispatch
//! (generics), so the missing explicit `Send` bound is not a concern here.
#![allow(async_fn_in_trait)]

pub mod checkpoint;
pub mod clock;
pub mod dedup;
pub mod delivery;
pub mod error;
pub mod publisher;
pub mod registry;
pub mod retry;
pub mod runner;
pub mod storage;

pub use checkpoint::{Checkpoint, CheckpointStore};
pub use clock::{Clock, FixedClock, SystemClock};
pub use dedup::CidDedup;
pub use delivery::{DeliveryClient, DeliveryMessage, HttpDelivery};
pub use error::{DeliveryError, IndexError, RegistryError, StorageError};
pub use publisher::{PublishMeta, PublishOutcome, Publisher};
pub use registry::{AnchorReceipt, FileRegistry, MockRegistry, RegistryClient};
pub use retry::RetryPolicy;
pub use runner::{BatchAnchorRunner, RunnerConfig, RunnerStats};
pub use storage::{HttpStorage, StorageClient};

// Re-export the shared types so downstream apps only need one dependency edge.
pub use wb_types;
