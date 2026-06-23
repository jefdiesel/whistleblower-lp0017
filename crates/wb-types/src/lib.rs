//! Shared Whistleblower types.
//!
//! This crate is the single source of truth for the data shapes that cross
//! component boundaries:
//!
//! * [`MetadataEnvelope`] — the document descriptor broadcast over Logos
//!   Delivery immediately after upload.
//! * [`metadata_hash`] — the canonical, language-agnostic 32-byte hash of an
//!   envelope. The on-chain registry stores this hash (not the envelope), and
//!   the batch-anchor tool transports `(cid, metadata_hash)` tuples.
//! * [`RegistryRecord`] — the per-document account payload stored on-chain by
//!   the LEZ registry program, Borsh-encoded.
//! * [`AnchorEntry`] — the `(cid, metadata_hash)` tuple accumulated by the
//!   batch-anchor tool and submitted to the registry.
//! * [`topic`] — the Logos Delivery content topic the app publishes to.
//!
//! The crate is deliberately dependency-light so the same definitions compile
//! both for host binaries and for the RISC0 zkVM guest (build the guest with
//! `default-features = false`).

mod envelope;
mod record;
pub mod topic;

#[cfg(feature = "hash")]
mod hash;

pub use envelope::{EnvelopeError, MetadataEnvelope, SCHEMA_VERSION};
pub use record::{AnchorEntry, RegistryRecord};

#[cfg(feature = "hash")]
pub use hash::{canonical_metadata_bytes, metadata_hash, META_HASH_DOMAIN};

/// A content identifier returned by Logos Storage (Codex-style multibase CID,
/// e.g. `zDvZRwzm8K7bcyPeBXcZzWD7AWc4VqNuseduDr3VsuYA1yXej49V`).
///
/// Kept as a `String` rather than a parsed type so the registry and delivery
/// layers stay agnostic to the exact multihash codec the Storage node emits.
pub type Cid = String;

/// Render a 32-byte hash as lowercase hex (`metadata_hash` display form).
pub fn hash_to_hex(h: &[u8; 32]) -> String {
    hex::encode(h)
}

/// Parse a lowercase/uppercase hex string back into a 32-byte hash.
pub fn hash_from_hex(s: &str) -> Result<[u8; 32], hex::FromHexError> {
    let mut out = [0u8; 32];
    hex::decode_to_slice(s.trim(), &mut out)?;
    Ok(out)
}
