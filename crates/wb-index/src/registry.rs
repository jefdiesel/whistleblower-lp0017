//! The on-chain CID registry abstraction.
//!
//! [`RegistryClient`] is the seam between the platform-agnostic indexing logic
//! and the LEZ-specific transaction machinery. The production implementation
//! (`LezRegistry`) lives in the separate `wb-lez-registry` crate so this crate
//! never pulls the RISC0/LEZ build dependencies; here we provide [`MockRegistry`],
//! an in-memory implementation used by unit/integration tests and the local demo.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use wb_types::{AnchorEntry, RegistryRecord};

use crate::error::RegistryError;

/// Result of an anchoring transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnchorReceipt {
    /// Transaction hash / identifier as reported by the chain.
    pub tx_hash: String,
    /// CIDs newly written by this transaction.
    pub anchored: Vec<String>,
    /// CIDs that were already registered (idempotent no-ops).
    pub already_present: Vec<String>,
}

impl AnchorReceipt {
    pub fn total(&self) -> usize {
        self.anchored.len() + self.already_present.len()
    }
}

/// Anchor `(cid, metadata_hash)` tuples on-chain and query the registry.
pub trait RegistryClient {
    /// Submit a batch of entries in a single transaction. Must be idempotent:
    /// re-submitting an already-registered CID succeeds (reported in
    /// [`AnchorReceipt::already_present`]) rather than failing.
    async fn anchor_batch(
        &self,
        entries: &[AnchorEntry],
        anchor_timestamp_ms: u64,
    ) -> Result<AnchorReceipt, RegistryError>;

    /// Look up the registry record for a CID, if anchored.
    async fn get_by_cid(&self, cid: &str) -> Result<Option<RegistryRecord>, RegistryError>;

    /// Convenience: anchor a single entry (the GUI's "anchor on-chain" action).
    async fn anchor_one(
        &self,
        entry: &AnchorEntry,
        anchor_timestamp_ms: u64,
    ) -> Result<AnchorReceipt, RegistryError> {
        self.anchor_batch(std::slice::from_ref(entry), anchor_timestamp_ms)
            .await
    }
}

/// In-memory registry mirroring the on-chain program's semantics (idempotent,
/// queryable by CID). Cloneable; all clones share one store.
#[derive(Clone, Default)]
pub struct MockRegistry {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    records: BTreeMap<String, RegistryRecord>,
    seq: u64,
}

impl MockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of distinct anchored CIDs.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Snapshot of all records (test helper).
    pub fn snapshot(&self) -> Vec<RegistryRecord> {
        self.inner
            .lock()
            .unwrap()
            .records
            .values()
            .cloned()
            .collect()
    }
}

impl RegistryClient for MockRegistry {
    async fn anchor_batch(
        &self,
        entries: &[AnchorEntry],
        anchor_timestamp_ms: u64,
    ) -> Result<AnchorReceipt, RegistryError> {
        let mut inner = self.inner.lock().unwrap();
        inner.seq += 1;
        let tx_hash = format!("mocktx-{:08x}", inner.seq);

        let mut anchored = Vec::new();
        let mut already_present = Vec::new();
        for e in entries {
            if inner.records.contains_key(&e.cid) {
                already_present.push(e.cid.clone());
            } else {
                inner.records.insert(
                    e.cid.clone(),
                    RegistryRecord::new(e.cid.clone(), e.metadata_hash, anchor_timestamp_ms),
                );
                anchored.push(e.cid.clone());
            }
        }
        Ok(AnchorReceipt {
            tx_hash,
            anchored,
            already_present,
        })
    }

    async fn get_by_cid(&self, cid: &str) -> Result<Option<RegistryRecord>, RegistryError> {
        Ok(self.inner.lock().unwrap().records.get(cid).cloned())
    }
}

/// On-disk JSON state for [`FileRegistry`].
#[derive(Serialize, Deserialize, Default)]
struct FileState {
    records: BTreeMap<String, RegistryRecord>,
    seq: u64,
}

/// A JSON-file-backed registry for **local/dev/CI use only** (not on-chain).
///
/// Unlike [`MockRegistry`], state survives across processes, so the local demo's
/// `anchor` and `query` commands work in separate CLI invocations without a
/// running sequencer. Production anchoring uses `LezRegistry` from the
/// `wb-lez-registry` crate. Semantics (idempotency, query-by-CID) mirror the
/// on-chain program.
#[derive(Clone, Debug)]
pub struct FileRegistry {
    path: PathBuf,
}

impl FileRegistry {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    async fn load(&self) -> Result<FileState, RegistryError> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| RegistryError::Decode(e.to_string()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(FileState::default()),
            Err(e) => Err(RegistryError::Transport(e.to_string())),
        }
    }

    async fn save(&self, st: &FileState) -> Result<(), RegistryError> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| RegistryError::Transport(e.to_string()))?;
            }
        }
        let bytes =
            serde_json::to_vec_pretty(st).map_err(|e| RegistryError::Decode(e.to_string()))?;
        let tmp = self.path.with_extension("json.tmp");
        tokio::fs::write(&tmp, &bytes)
            .await
            .map_err(|e| RegistryError::Transport(e.to_string()))?;
        tokio::fs::rename(&tmp, &self.path)
            .await
            .map_err(|e| RegistryError::Transport(e.to_string()))?;
        Ok(())
    }
}

impl RegistryClient for FileRegistry {
    async fn anchor_batch(
        &self,
        entries: &[AnchorEntry],
        anchor_timestamp_ms: u64,
    ) -> Result<AnchorReceipt, RegistryError> {
        let mut st = self.load().await?;
        st.seq += 1;
        let tx_hash = format!("filetx-{:08x}", st.seq);
        let mut anchored = Vec::new();
        let mut already_present = Vec::new();
        for e in entries {
            if st.records.contains_key(&e.cid) {
                already_present.push(e.cid.clone());
            } else {
                st.records.insert(
                    e.cid.clone(),
                    RegistryRecord::new(e.cid.clone(), e.metadata_hash, anchor_timestamp_ms),
                );
                anchored.push(e.cid.clone());
            }
        }
        self.save(&st).await?;
        Ok(AnchorReceipt {
            tx_hash,
            anchored,
            already_present,
        })
    }

    async fn get_by_cid(&self, cid: &str) -> Result<Option<RegistryRecord>, RegistryError> {
        Ok(self.load().await?.records.get(cid).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(cid: &str) -> AnchorEntry {
        AnchorEntry::new(cid, [7u8; 32])
    }

    #[tokio::test]
    async fn anchor_is_idempotent() {
        let reg = MockRegistry::new();
        let batch = vec![entry("zDvA"), entry("zDvB")];

        let r1 = reg.anchor_batch(&batch, 1000).await.unwrap();
        assert_eq!(r1.anchored.len(), 2);
        assert_eq!(r1.already_present.len(), 0);

        // Re-submit the same batch plus a new CID.
        let batch2 = vec![entry("zDvA"), entry("zDvB"), entry("zDvC")];
        let r2 = reg.anchor_batch(&batch2, 2000).await.unwrap();
        assert_eq!(r2.anchored, vec!["zDvC".to_string()]);
        assert_eq!(r2.already_present.len(), 2);
        assert_eq!(reg.len(), 3);
    }

    #[tokio::test]
    async fn query_returns_record() {
        let reg = MockRegistry::new();
        reg.anchor_batch(&[entry("zDvA")], 4242).await.unwrap();
        let rec = reg.get_by_cid("zDvA").await.unwrap().unwrap();
        assert_eq!(rec.cid, "zDvA");
        assert_eq!(rec.anchor_timestamp, 4242);
        assert!(reg.get_by_cid("missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn file_registry_persists_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.json");

        // First process writes A, B.
        let r1 = FileRegistry::new(&path);
        let rec = r1
            .anchor_batch(&[entry("zDvA"), entry("zDvB")], 100)
            .await
            .unwrap();
        assert_eq!(rec.anchored.len(), 2);

        // Second, independent handle (separate "process") sees them and dedups.
        let r2 = FileRegistry::new(&path);
        let rec2 = r2
            .anchor_batch(&[entry("zDvA"), entry("zDvC")], 200)
            .await
            .unwrap();
        assert_eq!(rec2.anchored, vec!["zDvC".to_string()]);
        assert_eq!(rec2.already_present, vec!["zDvA".to_string()]);
        assert_eq!(r2.get_by_cid("zDvB").await.unwrap().unwrap().cid, "zDvB");
    }
}
