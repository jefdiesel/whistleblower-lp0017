//! CID-level deduplication, used on both sides of Delivery:
//! * publisher-side, so re-broadcasting the same CID does not emit a duplicate
//!   envelope visible to subscribers;
//! * subscriber-side (in the batch runner), so an already-seen CID is not
//!   accumulated or anchored twice.

use std::collections::HashSet;

/// A set of CIDs already observed/handled.
#[derive(Default, Debug, Clone)]
pub struct CidDedup {
    seen: HashSet<String>,
}

impl CidDedup {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the set from a prior run (e.g. a checkpoint's anchored CIDs).
    pub fn seeded<I>(cids: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        Self {
            seen: cids.into_iter().collect(),
        }
    }

    /// Record a CID. Returns `true` if it was newly inserted (i.e. not a duplicate).
    pub fn insert(&mut self, cid: &str) -> bool {
        self.seen.insert(cid.to_owned())
    }

    pub fn contains(&self, cid: &str) -> bool {
        self.seen.contains(cid)
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_reports_novelty() {
        let mut d = CidDedup::new();
        assert!(d.insert("zDvA"));
        assert!(!d.insert("zDvA"));
        assert!(d.insert("zDvB"));
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn seeded_treats_known_as_duplicates() {
        let mut d = CidDedup::seeded(["zDvA".to_string()]);
        assert!(d.contains("zDvA"));
        assert!(!d.insert("zDvA"));
        assert!(d.insert("zDvC"));
    }
}
