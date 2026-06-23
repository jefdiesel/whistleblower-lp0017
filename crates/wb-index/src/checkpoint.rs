//! Durable checkpoint for the batch runner, so it resumes from the last
//! successfully anchored batch after a network interruption or restart without
//! re-processing already-registered CIDs.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Persisted progress of the batch-anchor tool.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    /// All CIDs known to be on-chain (anchored by this tool across runs). Seeds
    /// the dedup set on startup so they are never re-accumulated or re-submitted.
    #[serde(default)]
    pub anchored_cids: BTreeSet<String>,
    /// Highest Delivery message timestamp (ns) observed; informational.
    #[serde(default)]
    pub last_delivery_timestamp_ns: u64,
    /// Transaction hash of the most recently committed batch.
    #[serde(default)]
    pub last_batch_tx: Option<String>,
    /// Number of batches committed across the lifetime of this checkpoint.
    #[serde(default)]
    pub batches_committed: u64,
}

/// Reads/writes a [`Checkpoint`] to a JSON file with atomic (write-tmp+rename) saves.
#[derive(Clone, Debug)]
pub struct CheckpointStore {
    path: PathBuf,
}

impl CheckpointStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load the checkpoint, returning the default (empty) checkpoint if the file
    /// does not yet exist.
    pub async fn load(&self) -> io::Result<Checkpoint> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Checkpoint::default()),
            Err(e) => Err(e),
        }
    }

    /// Atomically persist the checkpoint (write to a temp file, then rename).
    pub async fn save(&self, cp: &Checkpoint) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        let bytes = serde_json::to_vec_pretty(cp)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let tmp = self.path.with_extension("json.tmp");
        tokio::fs::write(&tmp, &bytes).await?;
        tokio::fs::rename(&tmp, &self.path).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn load_missing_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().join("cp.json"));
        assert_eq!(store.load().await.unwrap(), Checkpoint::default());
    }

    #[tokio::test]
    async fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().join("nested/cp.json"));
        let mut cp = Checkpoint::default();
        cp.anchored_cids.insert("zDvA".into());
        cp.anchored_cids.insert("zDvB".into());
        cp.batches_committed = 2;
        cp.last_batch_tx = Some("mocktx-00000002".into());
        store.save(&cp).await.unwrap();
        assert_eq!(store.load().await.unwrap(), cp);
    }
}
