//! The permissionless batch-anchor loop.
//!
//! Subscribe to the Delivery topic, accumulate `(cid, metadata_hash)` tuples from
//! broadcast envelopes, and commit them to the registry in batches — idempotently
//! (re-anchoring a known CID is a no-op) and resumably (progress is checkpointed,
//! so an interrupted run continues without re-processing registered CIDs).
//!
//! Any party can run this against the public topic and registry; nothing here
//! requires coordination with the original publisher.

use std::future::Future;
use std::time::Duration;

use wb_types::{AnchorEntry, MetadataEnvelope};

use crate::checkpoint::{Checkpoint, CheckpointStore};
use crate::clock::{Clock, SystemClock};
use crate::dedup::CidDedup;
use crate::delivery::DeliveryClient;
use crate::error::IndexError;
use crate::registry::{AnchorReceipt, RegistryClient};

/// Tunables for [`BatchAnchorRunner`].
#[derive(Clone, Debug)]
pub struct RunnerConfig {
    pub content_topic: String,
    /// Flush as soon as this many tuples are pending. LP-0017 requires the
    /// registry to accept batches of at least 10; the default targets 50.
    pub batch_size: usize,
    /// Also flush a partial batch on this cadence.
    pub flush_interval: Duration,
    /// How often to drain the Delivery topic.
    pub poll_interval: Duration,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            content_topic: wb_types::topic::documents_content_topic(),
            batch_size: 50,
            flush_interval: Duration::from_secs(10),
            poll_interval: Duration::from_secs(2),
        }
    }
}

/// Cumulative counters for a run.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct RunnerStats {
    pub received: u64,
    pub accepted: u64,
    pub duplicates: u64,
    pub invalid: u64,
    pub anchored: u64,
    pub batches: u64,
}

struct RunState {
    pending: Vec<AnchorEntry>,
    dedup: CidDedup,
    checkpoint: Checkpoint,
    stats: RunnerStats,
    initialized: bool,
}

/// Drives the accumulate → batch-anchor → checkpoint loop.
pub struct BatchAnchorRunner<D, R, C = SystemClock> {
    delivery: D,
    registry: R,
    clock: C,
    config: RunnerConfig,
    store: CheckpointStore,
    state: RunState,
}

impl<D, R> BatchAnchorRunner<D, R, SystemClock>
where
    D: DeliveryClient,
    R: RegistryClient,
{
    pub fn new(delivery: D, registry: R, store: CheckpointStore, config: RunnerConfig) -> Self {
        Self::with_clock(delivery, registry, store, config, SystemClock)
    }
}

impl<D, R, C> BatchAnchorRunner<D, R, C>
where
    D: DeliveryClient,
    R: RegistryClient,
    C: Clock,
{
    pub fn with_clock(
        delivery: D,
        registry: R,
        store: CheckpointStore,
        config: RunnerConfig,
        clock: C,
    ) -> Self {
        Self {
            delivery,
            registry,
            clock,
            config,
            store,
            state: RunState {
                pending: Vec::new(),
                dedup: CidDedup::new(),
                checkpoint: Checkpoint::default(),
                stats: RunnerStats::default(),
                initialized: false,
            },
        }
    }

    pub fn stats(&self) -> &RunnerStats {
        &self.state.stats
    }

    pub fn checkpoint(&self) -> &Checkpoint {
        &self.state.checkpoint
    }

    pub fn pending_len(&self) -> usize {
        self.state.pending.len()
    }

    /// Load the checkpoint (seeding dedup with already-anchored CIDs) and
    /// subscribe to the Delivery topic. Idempotent.
    pub async fn init(&mut self) -> Result<(), IndexError> {
        if self.state.initialized {
            return Ok(());
        }
        self.state.checkpoint = self.store.load().await?;
        self.state.dedup = CidDedup::seeded(self.state.checkpoint.anchored_cids.iter().cloned());
        self.delivery.subscribe(&self.config.content_topic).await?;
        self.state.initialized = true;
        tracing::info!(
            topic = %self.config.content_topic,
            resumed_cids = self.state.checkpoint.anchored_cids.len(),
            "batch-anchor runner initialized"
        );
        Ok(())
    }

    /// Drain the topic once and accumulate new, valid, non-duplicate entries.
    /// Returns the number of entries accepted this poll.
    pub async fn poll_once(&mut self) -> Result<usize, IndexError> {
        let messages = self.delivery.poll(&self.config.content_topic).await?;
        let mut accepted = 0usize;
        for msg in messages {
            self.state.stats.received += 1;
            self.state.checkpoint.last_delivery_timestamp_ns = self
                .state
                .checkpoint
                .last_delivery_timestamp_ns
                .max(msg.timestamp_ns);

            let envelope = match MetadataEnvelope::from_json_bytes(&msg.payload) {
                Ok(e) => e,
                Err(e) => {
                    tracing::debug!(error = %e, "dropping non-envelope payload");
                    self.state.stats.invalid += 1;
                    continue;
                }
            };
            if envelope.validate().is_err() {
                self.state.stats.invalid += 1;
                continue;
            }
            if !self.state.dedup.insert(&envelope.cid) {
                self.state.stats.duplicates += 1;
                continue;
            }
            self.state.pending.push(envelope.anchor_entry());
            self.state.stats.accepted += 1;
            accepted += 1;
        }
        Ok(accepted)
    }

    /// Anchor all pending entries in a single batch and checkpoint the result.
    /// On registry failure the pending set is retained (so the next attempt
    /// re-submits — safe because anchoring is idempotent).
    pub async fn flush(&mut self) -> Result<Option<AnchorReceipt>, IndexError> {
        if self.state.pending.is_empty() {
            return Ok(None);
        }
        let ts = self.clock.now_ms();
        let receipt = self.registry.anchor_batch(&self.state.pending, ts).await?;

        // Every submitted CID is now on-chain (whether freshly anchored or
        // already present). Record them so we never reprocess them.
        for entry in &self.state.pending {
            self.state
                .checkpoint
                .anchored_cids
                .insert(entry.cid.clone());
        }
        self.state.checkpoint.batches_committed += 1;
        self.state.checkpoint.last_batch_tx = Some(receipt.tx_hash.clone());
        self.state.stats.anchored += receipt.anchored.len() as u64;
        self.state.stats.batches += 1;
        self.store.save(&self.state.checkpoint).await?;

        tracing::info!(
            tx = %receipt.tx_hash,
            anchored = receipt.anchored.len(),
            already_present = receipt.already_present.len(),
            "committed batch"
        );
        self.state.pending.clear();
        Ok(Some(receipt))
    }

    /// Run the accumulate/flush loop until `shutdown` resolves, then flush any
    /// remaining pending entries. Transient poll/flush errors are logged and the
    /// loop continues, so the daemon survives node downtime and resumes.
    pub async fn run<S>(&mut self, shutdown: S) -> Result<RunnerStats, IndexError>
    where
        S: Future<Output = ()>,
    {
        self.init().await?;

        let mut poll = tokio::time::interval(self.config.poll_interval);
        let mut flush = tokio::time::interval(self.config.flush_interval);
        // The first tick of an interval fires immediately; skip it.
        poll.tick().await;
        flush.tick().await;

        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                _ = poll.tick() => {
                    match self.poll_once().await {
                        Ok(_) => {
                            if self.state.pending.len() >= self.config.batch_size {
                                if let Err(e) = self.flush().await {
                                    tracing::warn!(error = %e, "flush failed; will retry");
                                }
                            }
                        }
                        Err(e) => tracing::warn!(error = %e, "poll failed; will retry"),
                    }
                }
                _ = flush.tick() => {
                    if let Err(e) = self.flush().await {
                        tracing::warn!(error = %e, "scheduled flush failed; will retry");
                    }
                }
            }
        }

        if let Err(e) = self.flush().await {
            tracing::warn!(error = %e, "final flush failed");
        }
        Ok(self.state.stats.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use crate::delivery::DeliveryMessage;
    use crate::error::DeliveryError;
    use crate::registry::MockRegistry;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use wb_types::MetadataEnvelope;

    /// In-memory delivery that hands back queued messages on each poll.
    #[derive(Default, Clone)]
    struct ScriptedDelivery {
        inbox: Arc<Mutex<VecDeque<DeliveryMessage>>>,
    }
    impl ScriptedDelivery {
        fn push_envelope(&self, env: &MetadataEnvelope) {
            self.inbox.lock().unwrap().push_back(DeliveryMessage {
                content_topic: wb_types::topic::documents_content_topic(),
                payload: env.to_json_bytes().unwrap(),
                timestamp_ns: env.timestamp.saturating_mul(1_000_000),
                message_hash: None,
            });
        }
    }
    impl DeliveryClient for ScriptedDelivery {
        async fn subscribe(&self, _t: &str) -> Result<(), DeliveryError> {
            Ok(())
        }
        async fn unsubscribe(&self, _t: &str) -> Result<(), DeliveryError> {
            Ok(())
        }
        async fn publish(
            &self,
            _t: &str,
            _p: &[u8],
            _ts: Option<u64>,
        ) -> Result<(), DeliveryError> {
            Ok(())
        }
        async fn poll(&self, _t: &str) -> Result<Vec<DeliveryMessage>, DeliveryError> {
            Ok(self.inbox.lock().unwrap().drain(..).collect())
        }
    }

    fn envelope(cid: &str) -> MetadataEnvelope {
        MetadataEnvelope::new(
            cid,
            "title",
            "desc",
            "text/plain",
            10,
            1_700_000_000_000,
            vec![],
        )
    }

    fn runner(
        delivery: ScriptedDelivery,
        registry: MockRegistry,
        store: CheckpointStore,
    ) -> BatchAnchorRunner<ScriptedDelivery, MockRegistry, FixedClock> {
        BatchAnchorRunner::with_clock(
            delivery,
            registry,
            store,
            RunnerConfig {
                batch_size: 10,
                ..RunnerConfig::default()
            },
            FixedClock(1_700_000_000_123),
        )
    }

    #[tokio::test]
    async fn accumulates_and_anchors_a_batch() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().join("cp.json"));
        let delivery = ScriptedDelivery::default();
        let registry = MockRegistry::new();

        for i in 0..12 {
            delivery.push_envelope(&envelope(&format!("zDv{i:02}")));
        }

        let mut r = runner(delivery, registry.clone(), store);
        r.init().await.unwrap();
        let accepted = r.poll_once().await.unwrap();
        assert_eq!(accepted, 12);
        assert_eq!(r.pending_len(), 12);

        let receipt = r.flush().await.unwrap().unwrap();
        assert_eq!(receipt.anchored.len(), 12);
        assert_eq!(registry.len(), 12);
        assert_eq!(r.pending_len(), 0);
        assert_eq!(r.checkpoint().anchored_cids.len(), 12);
        assert_eq!(r.checkpoint().batches_committed, 1);
        assert_eq!(r.stats().anchored, 12);
    }

    #[tokio::test]
    async fn empty_flush_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().join("cp.json"));
        let mut r = runner(ScriptedDelivery::default(), MockRegistry::new(), store);
        r.init().await.unwrap();
        assert!(r.flush().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn resumes_without_reprocessing() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().join("cp.json"));
        let registry = MockRegistry::new();

        // First run: anchor A, B, C.
        {
            let delivery = ScriptedDelivery::default();
            for cid in ["zDvA", "zDvB", "zDvC"] {
                delivery.push_envelope(&envelope(cid));
            }
            let mut r = runner(delivery, registry.clone(), store.clone());
            r.init().await.unwrap();
            r.poll_once().await.unwrap();
            r.flush().await.unwrap();
        }
        assert_eq!(registry.len(), 3);

        // Second run with a fresh runner sharing the same checkpoint + registry:
        // re-broadcasting the same CIDs must be treated as duplicates (resume).
        {
            let delivery = ScriptedDelivery::default();
            for cid in ["zDvA", "zDvB", "zDvC"] {
                delivery.push_envelope(&envelope(cid));
            }
            let mut r = runner(delivery, registry.clone(), store.clone());
            r.init().await.unwrap();
            let accepted = r.poll_once().await.unwrap();
            assert_eq!(
                accepted, 0,
                "previously anchored CIDs must not be reprocessed"
            );
            assert_eq!(r.stats().duplicates, 3);
            assert!(r.flush().await.unwrap().is_none());
        }
        // Registry unchanged — no duplicate rows.
        assert_eq!(registry.len(), 3);
    }

    #[tokio::test]
    async fn malformed_payloads_are_counted_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().join("cp.json"));
        let delivery = ScriptedDelivery::default();
        delivery.inbox.lock().unwrap().push_back(DeliveryMessage {
            content_topic: "t".into(),
            payload: b"not json".to_vec(),
            timestamp_ns: 1,
            message_hash: None,
        });
        let mut r = runner(delivery, MockRegistry::new(), store);
        r.init().await.unwrap();
        assert_eq!(r.poll_once().await.unwrap(), 0);
        assert_eq!(r.stats().invalid, 1);
    }
}
