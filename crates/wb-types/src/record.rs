use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// The `(cid, metadata_hash)` tuple the batch-anchor tool accumulates from the
/// Delivery topic and submits to the registry program in a single transaction.
///
/// `serde` is used for checkpoint/JSON tooling on the host; `borsh` matches the
/// deterministic encoding the LEZ program consumes for its instruction args.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AnchorEntry {
    pub cid: String,
    pub metadata_hash: [u8; 32],
}

impl AnchorEntry {
    pub fn new(cid: impl Into<String>, metadata_hash: [u8; 32]) -> Self {
        Self {
            cid: cid.into(),
            metadata_hash,
        }
    }
}

/// The per-document payload stored on-chain by the registry program, in the data
/// field of the document's program-derived account (PDA).
///
/// Borsh-encoded to match SPEL `#[account_type]` conventions; the query path
/// decodes raw account bytes back into this struct.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RegistryRecord {
    /// The content identifier this account anchors.
    pub cid: String,
    /// Canonical hash of the document's metadata envelope.
    pub metadata_hash: [u8; 32],
    /// Time the CID was anchored on-chain, Unix milliseconds (from the on-chain
    /// clock account; see the registry program README for the trust model).
    pub anchor_timestamp: u64,
}

impl RegistryRecord {
    pub fn new(cid: impl Into<String>, metadata_hash: [u8; 32], anchor_timestamp: u64) -> Self {
        Self {
            cid: cid.into(),
            metadata_hash,
            anchor_timestamp,
        }
    }
}
