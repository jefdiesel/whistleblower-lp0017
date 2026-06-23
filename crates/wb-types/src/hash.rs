//! Canonical metadata hashing.
//!
//! The registry stores `metadata_hash` rather than the full envelope, and the
//! batch-anchor tool transports it. To keep that hash reproducible across
//! languages (the Rust clients *and* the C++/QML Basecamp app), the encoding is
//! explicit and self-describing rather than relying on any serializer's
//! field/key ordering:
//!
//! ```text
//! metadata_hash = SHA256(
//!     LP("WB-META-v1")          // domain separator (encodes schema version)
//!  || LP(cid)
//!  || LP(title)
//!  || LP(description)
//!  || LP(content_type)
//!  || u64_le(size_bytes)
//!  || u64_le(timestamp)
//!  || u32_le(N)                 // N = number of *sorted, de-duplicated* tags
//!  || LP(tag_0) || .. || LP(tag_{N-1})
//! )
//!
//! where LP(s) = u32_le(byte_len(s)) || utf8_bytes(s)
//! ```
//!
//! Tags are sorted and de-duplicated before hashing, so two envelopes that
//! differ only in tag order or repeated tags hash identically. `schema_version`
//! is *not* hashed directly — it is bound through the `WB-META-v1` domain string.

use sha2::{Digest, Sha256};

use crate::envelope::MetadataEnvelope;

/// Domain separator. Bump in lock-step with [`crate::SCHEMA_VERSION`].
pub const META_HASH_DOMAIN: &str = "WB-META-v1";

fn put_lp(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
}

/// Produce the exact byte preimage that [`metadata_hash`] digests. Exposed so
/// other-language implementations can be tested against a known vector.
pub fn canonical_metadata_bytes(env: &MetadataEnvelope) -> Vec<u8> {
    let mut buf = Vec::new();
    put_lp(&mut buf, META_HASH_DOMAIN);
    put_lp(&mut buf, &env.cid);
    put_lp(&mut buf, &env.title);
    put_lp(&mut buf, &env.description);
    put_lp(&mut buf, &env.content_type);
    buf.extend_from_slice(&env.size_bytes.to_le_bytes());
    buf.extend_from_slice(&env.timestamp.to_le_bytes());

    let mut tags: Vec<&str> = env.tags.iter().map(|s| s.as_str()).collect();
    tags.sort_unstable();
    tags.dedup();
    buf.extend_from_slice(&(tags.len() as u32).to_le_bytes());
    for t in tags {
        put_lp(&mut buf, t);
    }
    buf
}

/// The canonical 32-byte SHA-256 metadata hash for `env`.
pub fn metadata_hash(env: &MetadataEnvelope) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(canonical_metadata_bytes(env));
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MetadataEnvelope {
        MetadataEnvelope::new(
            "zDvSampleCidAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "Leaked memo",
            "Internal memo describing X",
            "application/pdf",
            12_345,
            1_700_000_000_000,
            vec!["leak".into(), "memo".into()],
        )
    }

    #[test]
    fn hash_is_deterministic() {
        assert_eq!(metadata_hash(&sample()), metadata_hash(&sample()));
    }

    #[test]
    fn hash_ignores_tag_order_and_duplicates() {
        let mut a = sample();
        a.tags = vec!["memo".into(), "leak".into(), "leak".into()];
        let mut b = sample();
        b.tags = vec!["leak".into(), "memo".into()];
        assert_eq!(metadata_hash(&a), metadata_hash(&b));
    }

    #[test]
    fn hash_changes_with_content() {
        let mut other = sample();
        other.size_bytes += 1;
        assert_ne!(metadata_hash(&sample()), metadata_hash(&other));

        let mut other2 = sample();
        other2.cid = "zDvDifferentCid".into();
        assert_ne!(metadata_hash(&sample()), metadata_hash(&other2));
    }

    #[test]
    fn anchor_entry_matches_hash() {
        let env = sample();
        assert_eq!(env.anchor_entry().metadata_hash, metadata_hash(&env));
        assert_eq!(env.anchor_entry().cid, env.cid);
    }

    #[test]
    fn json_roundtrip_preserves_hash() {
        let env = sample();
        let bytes = env.to_json_bytes().unwrap();
        let back = MetadataEnvelope::from_json_bytes(&bytes).unwrap();
        assert_eq!(env, back);
        assert_eq!(metadata_hash(&env), metadata_hash(&back));
    }
}
