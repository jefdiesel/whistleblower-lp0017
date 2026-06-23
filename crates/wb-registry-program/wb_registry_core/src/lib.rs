//! Shared types and PDA derivation for the Whistleblower on-chain registry.
//!
//! These definitions are the contract between the on-chain guest program
//! (`methods/guest/src/bin/wb_registry.rs`) and the off-chain host adapter
//! (`crates/wb-lez-registry`). The guest re-declares `AnchorArg`/`RegistryRecord`
//! with `#[account_type]` for IDL generation; the **borsh field order/types here
//! MUST stay identical** so the two encodings are wire-compatible.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Domain separator for the per-CID PDA seed.
pub const CID_PDA_DOMAIN: &[u8] = b"WB-CID-PDA-v1";

/// Derive the 32-byte public-PDA seed for a CID.
///
/// CIDs (e.g. base32 CIDv1, ~59 chars) exceed the 32-byte PDA seed limit, so the
/// CID is hashed. The guest (when claiming/validating the PDA) and the host
/// (when deriving the account to query) MUST use this identical derivation, so
/// it lives here as the single source of truth.
pub fn cid_seed(cid: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(CID_PDA_DOMAIN);
    hasher.update(cid.as_bytes());
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// One `(cid, metadata_hash)` tuple — the unit the batch instruction anchors.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct AnchorArg {
    pub cid: String,
    pub metadata_hash: [u8; 32],
}

/// The per-document payload stored in each registry PDA's account data.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct RegistryRecord {
    pub cid: String,
    pub metadata_hash: [u8; 32],
    pub anchor_timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cid_seed_is_deterministic_and_distinct() {
        assert_eq!(cid_seed("zDvAAA"), cid_seed("zDvAAA"));
        assert_ne!(cid_seed("zDvAAA"), cid_seed("zDvBBB"));
    }

    #[test]
    fn record_borsh_roundtrips() {
        let r = RegistryRecord {
            cid: "zDvAAA".into(),
            metadata_hash: [9u8; 32],
            anchor_timestamp: 1234,
        };
        let bytes = borsh::to_vec(&r).unwrap();
        let back: RegistryRecord = borsh::from_slice(&bytes).unwrap();
        assert_eq!(r, back);
    }
}
