//! The publish half of the pipeline: upload bytes to Storage (with retry), then
//! broadcast a metadata envelope over Delivery (deduplicated by CID).

use tokio::sync::Mutex;
use wb_types::{topic, Cid, MetadataEnvelope};

use crate::clock::{Clock, SystemClock};
use crate::dedup::CidDedup;
use crate::delivery::DeliveryClient;
use crate::error::IndexError;
use crate::retry::RetryPolicy;
use crate::storage::StorageClient;

/// Caller-supplied metadata for a publish.
#[derive(Clone, Debug, Default)]
pub struct PublishMeta {
    pub title: String,
    pub description: String,
    /// MIME type. If `None`, inferred from `filename`, else `application/octet-stream`.
    pub content_type: Option<String>,
    /// Original filename (used for content-type inference and the upload header).
    pub filename: Option<String>,
    pub tags: Vec<String>,
}

/// Outcome of [`Publisher::publish`].
#[derive(Clone, Debug)]
pub struct PublishOutcome {
    pub cid: Cid,
    pub envelope: MetadataEnvelope,
    /// `false` if the CID had already been broadcast and was deduplicated.
    pub broadcast: bool,
}

/// Drives upload + broadcast. Generic over the Storage/Delivery implementations
/// and the clock so it is trivially testable.
pub struct Publisher<S, D, C = SystemClock> {
    storage: S,
    delivery: D,
    clock: C,
    content_topic: String,
    retry: RetryPolicy,
    seen: Mutex<CidDedup>,
}

impl<S, D> Publisher<S, D, SystemClock>
where
    S: StorageClient,
    D: DeliveryClient,
{
    /// Construct with sensible defaults: system clock, the documents content
    /// topic, and the default retry policy.
    pub fn new(storage: S, delivery: D) -> Self {
        Self {
            storage,
            delivery,
            clock: SystemClock,
            content_topic: topic::documents_content_topic(),
            retry: RetryPolicy::default(),
            seen: Mutex::new(CidDedup::new()),
        }
    }
}

impl<S, D, C> Publisher<S, D, C>
where
    S: StorageClient,
    D: DeliveryClient,
    C: Clock,
{
    pub fn with_clock(storage: S, delivery: D, clock: C) -> Self {
        Self {
            storage,
            delivery,
            clock,
            content_topic: topic::documents_content_topic(),
            retry: RetryPolicy::default(),
            seen: Mutex::new(CidDedup::new()),
        }
    }

    pub fn content_topic(mut self, topic: impl Into<String>) -> Self {
        self.content_topic = topic.into();
        self
    }

    pub fn retry_policy(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    pub fn topic(&self) -> &str {
        &self.content_topic
    }

    /// Upload bytes to Storage, retrying transient failures with exponential
    /// backoff and surfacing a clear error after exhausting retries.
    pub async fn upload(
        &self,
        bytes: &[u8],
        content_type: Option<&str>,
        filename: Option<&str>,
    ) -> Result<Cid, IndexError> {
        let mut attempt: u32 = 0;
        loop {
            match self.storage.upload(bytes, content_type, filename).await {
                Ok(cid) => return Ok(cid),
                Err(e) => {
                    attempt += 1;
                    if attempt > self.retry.max_retries || !e.is_transient() {
                        return Err(IndexError::UploadRetriesExhausted {
                            attempts: attempt,
                            source: e,
                        });
                    }
                    tracing::warn!(attempt, error = %e, "storage upload failed; backing off");
                    tokio::time::sleep(self.retry.delay_for(attempt)).await;
                }
            }
        }
    }

    /// Broadcast an envelope over Delivery, deduplicated by CID. Returns whether
    /// the envelope was actually published (`false` = already broadcast).
    pub async fn broadcast(&self, envelope: &MetadataEnvelope) -> Result<bool, IndexError> {
        envelope.validate()?;
        {
            let mut seen = self.seen.lock().await;
            if !seen.insert(&envelope.cid) {
                tracing::debug!(cid = %envelope.cid, "skipping duplicate broadcast");
                return Ok(false);
            }
        }
        let payload = envelope.to_json_bytes()?;
        self.delivery
            .publish(&self.content_topic, &payload, Some(self.clock.now_ns()))
            .await?;
        Ok(true)
    }

    /// Full publish flow: upload, build the envelope, and broadcast it.
    pub async fn publish(
        &self,
        bytes: &[u8],
        meta: PublishMeta,
    ) -> Result<PublishOutcome, IndexError> {
        let content_type = meta
            .content_type
            .clone()
            .unwrap_or_else(|| infer_content_type(meta.filename.as_deref()));

        let cid = self
            .upload(bytes, Some(&content_type), meta.filename.as_deref())
            .await?;

        let envelope = MetadataEnvelope::new(
            cid.clone(),
            meta.title,
            meta.description,
            content_type,
            bytes.len() as u64,
            self.clock.now_ms(),
            meta.tags,
        );

        let broadcast = self.broadcast(&envelope).await?;
        Ok(PublishOutcome {
            cid,
            envelope,
            broadcast,
        })
    }
}

/// Best-effort MIME inference from a filename extension.
pub fn infer_content_type(filename: Option<&str>) -> String {
    let ext = filename
        .and_then(|f| f.rsplit('.').next())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    let mime = match ext.as_str() {
        "pdf" => "application/pdf",
        "txt" | "log" | "md" => "text/plain",
        "json" => "application/json",
        "csv" => "text/csv",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "zip" => "application/zip",
        "mp4" => "video/mp4",
        "html" | "htm" => "text/html",
        _ => "application/octet-stream",
    };
    mime.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use crate::delivery::{DeliveryMessage, HttpDelivery};
    use crate::error::DeliveryError;
    use crate::storage::HttpStorage;
    use std::sync::{Arc, Mutex as StdMutex};

    #[test]
    fn content_type_inference() {
        assert_eq!(infer_content_type(Some("a.pdf")), "application/pdf");
        assert_eq!(infer_content_type(Some("README.MD")), "text/plain");
        assert_eq!(
            infer_content_type(Some("x.unknown")),
            "application/octet-stream"
        );
        assert_eq!(infer_content_type(None), "application/octet-stream");
    }

    // A failing storage that always returns a transient error, counting calls.
    struct FlakyStorage {
        calls: StdMutex<u32>,
        succeed_after: u32,
    }
    impl StorageClient for FlakyStorage {
        async fn upload(
            &self,
            _b: &[u8],
            _ct: Option<&str>,
            _f: Option<&str>,
        ) -> Result<Cid, crate::error::StorageError> {
            let mut c = self.calls.lock().unwrap();
            *c += 1;
            if *c >= self.succeed_after {
                Ok(format!("zDvOk{c}"))
            } else {
                Err(crate::error::StorageError::Status {
                    status: 503,
                    body: "busy".into(),
                })
            }
        }
        async fn download(&self, _cid: &str) -> Result<Vec<u8>, crate::error::StorageError> {
            unreachable!()
        }
    }

    // A delivery that records published payloads in memory. Cloneable so a test
    // can keep a handle after the publisher takes ownership.
    #[derive(Default, Clone)]
    struct RecordingDelivery {
        published: Arc<StdMutex<Vec<Vec<u8>>>>,
    }
    impl DeliveryClient for RecordingDelivery {
        async fn subscribe(&self, _t: &str) -> Result<(), DeliveryError> {
            Ok(())
        }
        async fn unsubscribe(&self, _t: &str) -> Result<(), DeliveryError> {
            Ok(())
        }
        async fn publish(
            &self,
            _t: &str,
            payload: &[u8],
            _ts: Option<u64>,
        ) -> Result<(), DeliveryError> {
            self.published.lock().unwrap().push(payload.to_vec());
            Ok(())
        }
        async fn poll(&self, _t: &str) -> Result<Vec<DeliveryMessage>, DeliveryError> {
            Ok(vec![])
        }
    }

    #[tokio::test(start_paused = true)]
    async fn upload_retries_then_succeeds() {
        let storage = FlakyStorage {
            calls: StdMutex::new(0),
            succeed_after: 3,
        };
        let pubr = Publisher::with_clock(storage, RecordingDelivery::default(), FixedClock(1))
            .retry_policy(RetryPolicy {
                max_retries: 5,
                base_delay: std::time::Duration::from_millis(1),
                ..RetryPolicy::default()
            });
        let cid = pubr
            .upload(b"hello", Some("text/plain"), None)
            .await
            .unwrap();
        assert_eq!(cid, "zDvOk3");
    }

    #[tokio::test(start_paused = true)]
    async fn upload_exhausts_with_clear_error() {
        let storage = FlakyStorage {
            calls: StdMutex::new(0),
            succeed_after: 999,
        };
        let pubr = Publisher::with_clock(storage, RecordingDelivery::default(), FixedClock(1))
            .retry_policy(RetryPolicy {
                max_retries: 2,
                base_delay: std::time::Duration::from_millis(1),
                ..RetryPolicy::default()
            });
        let err = pubr.upload(b"x", None, None).await.unwrap_err();
        match err {
            IndexError::UploadRetriesExhausted { attempts, .. } => assert_eq!(attempts, 3),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn publish_then_dedup_broadcast() {
        // Storage returns a fixed CID so two publishes collide.
        struct FixedStorage;
        impl StorageClient for FixedStorage {
            async fn upload(
                &self,
                _b: &[u8],
                _ct: Option<&str>,
                _f: Option<&str>,
            ) -> Result<Cid, crate::error::StorageError> {
                Ok("zDvFixed".into())
            }
            async fn download(&self, _c: &str) -> Result<Vec<u8>, crate::error::StorageError> {
                unreachable!()
            }
        }
        let delivery = RecordingDelivery::default();
        let handle = delivery.clone();
        let pubr = Publisher::with_clock(FixedStorage, delivery, FixedClock(1_700_000_000_000));

        let meta = PublishMeta {
            title: "t".into(),
            content_type: Some("text/plain".into()),
            ..Default::default()
        };
        let o1 = pubr.publish(b"a", meta.clone()).await.unwrap();
        assert!(o1.broadcast);
        let o2 = pubr.publish(b"a", meta).await.unwrap();
        assert!(
            !o2.broadcast,
            "second publish of same CID must be deduplicated"
        );

        // Exactly one payload should have hit the wire.
        assert_eq!(handle.published.lock().unwrap().len(), 1);
    }

    // Compile-time check that the real HTTP clients satisfy the traits.
    fn _assert_impls() {
        fn is_storage<T: StorageClient>() {}
        fn is_delivery<T: DeliveryClient>() {}
        is_storage::<HttpStorage>();
        is_delivery::<HttpDelivery>();
    }
}
