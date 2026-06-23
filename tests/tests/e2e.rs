//! End-to-end integration tests for the Whistleblower indexing pipeline
//! (Logos prize LP-0017).
//!
//! These tests treat `wb-index` as a black box and wire the **full** pipeline
//! together with in-process fakes:
//!
//! ```text
//!   bytes --> Publisher.upload (InMemoryStorage, content-addressed CID)
//!         --> Publisher.broadcast (InMemoryDelivery, a shared queue)
//!         --> BatchAnchorRunner.poll_once (drains the SAME queue)
//!         --> BatchAnchorRunner.flush (MockRegistry, idempotent on-chain stand-in)
//!         --> RegistryClient.get_by_cid (query back what was anchored)
//! ```
//!
//! The fakes are deliberately minimal but faithful:
//!
//! * [`InMemoryStorage`] is content-addressed: identical bytes always yield the
//!   same CID (mirroring a real CID-based store), which is what makes the
//!   dedup-by-CID behaviour observable end to end.
//! * [`InMemoryDelivery`] is a shared `Arc<Mutex<VecDeque<DeliveryMessage>>>`:
//!   `publish` pushes, `poll` drains. This is the accumulation primitive the
//!   batch runner relies on, and being `Clone` lets the publisher and the runner
//!   share one queue even though each takes its delivery client by value.
//!
//! A final `#[ignore]`d test (`real_nodes_smoke`) drives the real REST clients
//! against live Storage/Delivery nodes when `WB_STORAGE_URL` / `WB_DELIVERY_URL`
//! are set; see that test for manual-run instructions.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use wb_index::{
    BatchAnchorRunner, CheckpointStore, DeliveryClient, DeliveryError, MockRegistry, PublishMeta,
    Publisher, RegistryClient, RunnerConfig, StorageClient, StorageError,
};
use wb_types::topic::documents_content_topic;

// ---------------------------------------------------------------------------
// In-memory fakes
// ---------------------------------------------------------------------------

/// A content-addressed, in-memory [`StorageClient`].
///
/// The CID is a deterministic function of the bytes (`"zDv" + hex(fnv1a(bytes))`),
/// so uploading the same content twice returns the same CID — exactly the
/// property the publisher's broadcast-dedup and the runner's anchor-dedup rely
/// on. Uploaded bytes are retained so `download` can round-trip them.
#[derive(Clone, Default)]
struct InMemoryStorage {
    blobs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl InMemoryStorage {
    fn new() -> Self {
        Self::default()
    }

    /// A small, dependency-free deterministic hash (64-bit FNV-1a) rendered as a
    /// CID-shaped string. Identical bytes -> identical CID.
    fn cid_for(bytes: &[u8]) -> String {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in bytes {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        format!("zDv{hash:016x}")
    }
}

impl StorageClient for InMemoryStorage {
    async fn upload(
        &self,
        bytes: &[u8],
        _content_type: Option<&str>,
        _filename: Option<&str>,
    ) -> Result<String, StorageError> {
        let cid = Self::cid_for(bytes);
        self.blobs
            .lock()
            .unwrap()
            .insert(cid.clone(), bytes.to_vec());
        Ok(cid)
    }

    async fn download(&self, cid: &str) -> Result<Vec<u8>, StorageError> {
        self.blobs
            .lock()
            .unwrap()
            .get(cid)
            .cloned()
            .ok_or(StorageError::EmptyCid)
    }
}

/// An in-memory [`DeliveryClient`] backed by a single shared queue.
///
/// `publish` pushes a [`DeliveryMessage`]; `poll` drains everything buffered
/// since the last poll. `subscribe`/`unsubscribe` are no-ops. Cloning shares the
/// same underlying queue, so the publisher and the batch runner can be handed
/// independent clones that nonetheless talk to one another.
#[derive(Clone, Default)]
struct InMemoryDelivery {
    queue: Arc<Mutex<VecDeque<wb_index::DeliveryMessage>>>,
}

impl InMemoryDelivery {
    fn new() -> Self {
        Self::default()
    }
}

impl DeliveryClient for InMemoryDelivery {
    async fn subscribe(&self, _content_topic: &str) -> Result<(), DeliveryError> {
        Ok(())
    }

    async fn unsubscribe(&self, _content_topic: &str) -> Result<(), DeliveryError> {
        Ok(())
    }

    async fn publish(
        &self,
        content_topic: &str,
        payload: &[u8],
        timestamp_ns: Option<u64>,
    ) -> Result<(), DeliveryError> {
        self.queue
            .lock()
            .unwrap()
            .push_back(wb_index::DeliveryMessage {
                content_topic: content_topic.to_string(),
                payload: payload.to_vec(),
                timestamp_ns: timestamp_ns.unwrap_or(0),
                message_hash: None,
            });
        Ok(())
    }

    async fn poll(
        &self,
        _content_topic: &str,
    ) -> Result<Vec<wb_index::DeliveryMessage>, DeliveryError> {
        Ok(self.queue.lock().unwrap().drain(..).collect())
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Build distinct publish metadata + bytes for document `i`.
fn doc(i: usize) -> (Vec<u8>, PublishMeta) {
    let bytes = format!("whistleblower document #{i}: classified payload {i}").into_bytes();
    let meta = PublishMeta {
        title: format!("Document {i}"),
        description: format!("Leaked record number {i}"),
        content_type: Some("text/plain".to_string()),
        filename: Some(format!("doc-{i}.txt")),
        tags: vec!["leak".to_string(), format!("batch-{}", i % 3)],
    };
    (bytes, meta)
}

// ---------------------------------------------------------------------------
// Test 1: the full pipeline, upload -> broadcast -> batch-anchor -> query
// ---------------------------------------------------------------------------

#[tokio::test]
async fn full_pipeline_upload_broadcast_batch_anchor_query() -> anyhow::Result<()> {
    const N: usize = 12;

    let storage = InMemoryStorage::new();
    let delivery = InMemoryDelivery::new();
    let registry = MockRegistry::new();

    // --- Publish side: upload + broadcast N distinct documents. -------------
    // The runner reads from the SAME delivery queue, so hand it a clone.
    let publisher =
        Publisher::new(storage.clone(), delivery.clone()).content_topic(documents_content_topic());

    let mut envelopes = Vec::with_capacity(N);
    let mut cids = Vec::with_capacity(N);
    for i in 0..N {
        let (bytes, meta) = doc(i);
        let outcome = publisher.publish(&bytes, meta).await?;
        assert!(
            outcome.broadcast,
            "doc {i} is distinct and must actually be broadcast"
        );
        cids.push(outcome.cid.clone());
        envelopes.push(outcome.envelope);
    }

    // All CIDs distinct (content-addressed over distinct bytes).
    let unique: std::collections::HashSet<&String> = cids.iter().collect();
    assert_eq!(
        unique.len(),
        N,
        "every distinct document must get a distinct CID"
    );

    // --- Anchor side: drain the queue and commit a batch. -------------------
    let tmp = tempfile::tempdir()?;
    let checkpoint_path = tmp.path().join("checkpoint.json");
    let mut runner = BatchAnchorRunner::new(
        delivery.clone(),
        registry.clone(),
        CheckpointStore::new(&checkpoint_path),
        RunnerConfig {
            content_topic: documents_content_topic(),
            batch_size: 10,
            ..RunnerConfig::default()
        },
    );

    runner.init().await?;
    let accepted = runner.poll_once().await?;
    assert_eq!(
        accepted, N,
        "runner must accept all {N} broadcast envelopes"
    );
    assert!(
        runner.pending_len() >= 10,
        "a batch of at least 10 (LP-0017 minimum) must be staged before flush; got {}",
        runner.pending_len()
    );

    let receipt = runner.flush().await?;
    let receipt = receipt.expect("flush with pending entries must produce a receipt");
    assert_eq!(
        receipt.anchored.len(),
        N,
        "all {N} CIDs must be freshly anchored in this batch"
    );
    assert!(
        receipt.already_present.is_empty(),
        "nothing was anchored before, so already_present must be empty"
    );
    // The single submitted batch carried >= 10 entries (LP-0017 requirement).
    assert!(
        receipt.total() >= 10,
        "the submitted batch must contain at least 10 entries; got {}",
        receipt.total()
    );

    // --- Query side: the registry holds exactly N records, queryable by CID. -
    assert_eq!(
        registry.len(),
        N,
        "registry must hold all {N} anchored CIDs"
    );

    // For several CIDs, the on-chain record's metadata_hash must equal the
    // canonical hash of the envelope that was published — proving the hash is
    // carried faithfully end to end.
    for i in [0usize, 3, 7, N - 1] {
        let env = &envelopes[i];
        let record = registry
            .get_by_cid(&env.cid)
            .await?
            .unwrap_or_else(|| panic!("CID for doc {i} must be queryable from the registry"));
        assert_eq!(record.cid, env.cid);
        assert_eq!(
            record.metadata_hash,
            env.metadata_hash(),
            "anchored metadata_hash for doc {i} must equal the published envelope's hash"
        );
    }

    // Pipeline-level bookkeeping sanity checks.
    assert_eq!(runner.pending_len(), 0, "flush must clear the pending set");
    assert_eq!(runner.checkpoint().anchored_cids.len(), N);
    assert_eq!(runner.checkpoint().batches_committed, 1);
    assert_eq!(runner.stats().accepted, N as u64);
    assert_eq!(runner.stats().anchored, N as u64);
    assert_eq!(runner.stats().batches, 1);

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 2: resume is idempotent (no reprocessing across runs)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resume_is_idempotent() -> anyhow::Result<()> {
    const N: usize = 12;

    let storage = InMemoryStorage::new();
    let registry = MockRegistry::new();

    let tmp = tempfile::tempdir()?;
    let checkpoint_path = tmp.path().join("checkpoint.json");

    // Helper to publish the same N documents into a fresh delivery queue.
    let publish_all = || async {
        let delivery = InMemoryDelivery::new();
        let publisher = Publisher::new(storage.clone(), delivery.clone());
        for i in 0..N {
            let (bytes, meta) = doc(i);
            publisher.publish(&bytes, meta).await?;
        }
        Ok::<_, anyhow::Error>(delivery)
    };

    // --- First run: anchor all N. ------------------------------------------
    {
        let delivery = publish_all().await?;
        let mut runner = BatchAnchorRunner::new(
            delivery,
            registry.clone(),
            CheckpointStore::new(&checkpoint_path),
            RunnerConfig {
                batch_size: 10,
                ..RunnerConfig::default()
            },
        );
        runner.init().await?;
        assert_eq!(runner.poll_once().await?, N);
        let receipt = runner
            .flush()
            .await?
            .expect("first run must anchor a batch");
        assert_eq!(receipt.anchored.len(), N);
    }
    assert_eq!(registry.len(), N, "first run must populate the registry");

    // --- Second run: fresh runner, SAME checkpoint path + SAME registry. ----
    // The same envelopes are re-broadcast; resume must treat every one as a
    // duplicate (the checkpoint seeded the dedup set), accept 0, and not flush.
    {
        let delivery = publish_all().await?;
        let mut runner = BatchAnchorRunner::new(
            delivery,
            registry.clone(),
            CheckpointStore::new(&checkpoint_path),
            RunnerConfig {
                batch_size: 10,
                ..RunnerConfig::default()
            },
        );
        runner.init().await?;
        let accepted = runner.poll_once().await?;
        assert_eq!(
            accepted, 0,
            "previously anchored CIDs must not be reprocessed on resume"
        );
        assert_eq!(runner.stats().duplicates, N as u64);
        assert_eq!(runner.pending_len(), 0);
        assert!(
            runner.flush().await?.is_none(),
            "nothing pending, so flush must be a no-op"
        );
    }

    // Registry unchanged: no duplicate rows written.
    assert_eq!(
        registry.len(),
        N,
        "resume must not write any duplicate registry rows"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 3: broadcast dedup (identical bytes broadcast once)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dedup_broadcast() -> anyhow::Result<()> {
    let storage = InMemoryStorage::new();
    let delivery = InMemoryDelivery::new();
    let publisher = Publisher::new(storage, delivery.clone());

    let bytes = b"identical classified payload";
    let meta = PublishMeta {
        title: "Same document".to_string(),
        content_type: Some("text/plain".to_string()),
        ..Default::default()
    };

    // First publish of these bytes: actually broadcast.
    let first = publisher.publish(bytes, meta.clone()).await?;
    assert!(first.broadcast, "first publish must be broadcast");

    // Second publish of the SAME bytes -> same content-addressed CID -> the
    // publisher deduplicates and does not re-broadcast.
    let second = publisher.publish(bytes, meta).await?;
    assert!(
        !second.broadcast,
        "re-publishing identical bytes (same CID) must be deduplicated"
    );
    assert_eq!(
        first.cid, second.cid,
        "identical bytes must produce one CID"
    );

    // Exactly one envelope should have hit the wire.
    let drained = delivery.poll(&documents_content_topic()).await?;
    assert_eq!(drained.len(), 1, "only one envelope may be broadcast");

    Ok(())
}

// ---------------------------------------------------------------------------
// Optional smoke test against REAL Logos nodes (ignored by default)
// ---------------------------------------------------------------------------

/// Smoke-test the real REST clients against live Storage + Delivery nodes.
///
/// Ignored by default because it requires running nodes. To run it manually:
///
/// ```bash
/// export WB_STORAGE_URL="http://localhost:8080/api/storage/v1"
/// export WB_DELIVERY_URL="http://127.0.0.1:8645"
/// cargo test -p wb-e2e --test e2e -- --ignored real_nodes_smoke --nocapture
/// ```
///
/// If either environment variable is unset the test exits early (a no-op), so it
/// is safe to invoke `--ignored` in environments without live nodes.
#[tokio::test]
#[ignore = "requires live Logos Storage/Delivery nodes; set WB_STORAGE_URL and WB_DELIVERY_URL"]
async fn real_nodes_smoke() -> anyhow::Result<()> {
    use wb_index::{HttpDelivery, HttpStorage};

    let (Ok(storage_url), Ok(delivery_url)) = (
        std::env::var("WB_STORAGE_URL"),
        std::env::var("WB_DELIVERY_URL"),
    ) else {
        eprintln!("real_nodes_smoke: WB_STORAGE_URL / WB_DELIVERY_URL not set; skipping");
        return Ok(());
    };

    let publisher = Publisher::new(
        HttpStorage::new(storage_url),
        HttpDelivery::new(delivery_url),
    );

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let payload = format!("whistleblower e2e smoke {nonce}");
    let meta = PublishMeta {
        title: "e2e smoke".to_string(),
        description: "tiny payload from real_nodes_smoke".to_string(),
        content_type: Some("text/plain".to_string()),
        filename: Some("smoke.txt".to_string()),
        tags: vec!["smoke".to_string()],
    };

    let outcome = publisher.publish(payload.as_bytes(), meta).await?;
    assert!(!outcome.cid.is_empty(), "live storage must return a CID");
    assert!(outcome.broadcast, "fresh payload must be broadcast");
    eprintln!("real_nodes_smoke: uploaded + broadcast CID {}", outcome.cid);

    Ok(())
}
