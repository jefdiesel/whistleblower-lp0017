use serde::{Deserialize, Serialize};

use crate::record::AnchorEntry;

/// Wire/schema version of [`MetadataEnvelope`]. Bumping this requires a matching
/// new [`crate::META_HASH_DOMAIN`] so old and new hashes never collide.
pub const SCHEMA_VERSION: u16 = 1;

fn default_schema_version() -> u16 {
    SCHEMA_VERSION
}

/// The descriptor broadcast over Logos Delivery immediately after a document is
/// uploaded to Logos Storage.
///
/// Serialized as JSON for the Delivery payload (human-inspectable and trivial to
/// reproduce from the C++/QML Basecamp app). The minimum field set is mandated
/// by LP-0017: `cid`, `title`, `description`, `content_type`, `size_bytes`,
/// `timestamp`, and an optional `tags` list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataEnvelope {
    /// Logos Storage content identifier for the uploaded bytes.
    pub cid: String,
    /// Human title of the document.
    pub title: String,
    /// Free-form description. May be empty.
    #[serde(default)]
    pub description: String,
    /// MIME type, e.g. `application/pdf`.
    pub content_type: String,
    /// Size of the uploaded bytes.
    pub size_bytes: u64,
    /// Publication time, Unix milliseconds (UTC).
    pub timestamp: u64,
    /// Optional free-form tags. Order- and duplicate-insensitive for hashing.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Envelope schema version. Defaults to [`SCHEMA_VERSION`] when absent.
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
}

/// Errors produced while validating an envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvelopeError {
    EmptyCid,
    EmptyContentType,
    UnsupportedSchemaVersion(u16),
}

impl core::fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EnvelopeError::EmptyCid => write!(f, "envelope cid is empty"),
            EnvelopeError::EmptyContentType => write!(f, "envelope content_type is empty"),
            EnvelopeError::UnsupportedSchemaVersion(v) => {
                write!(f, "unsupported envelope schema_version {v}")
            }
        }
    }
}

impl std::error::Error for EnvelopeError {}

impl MetadataEnvelope {
    /// Construct an envelope stamping the current schema version.
    pub fn new(
        cid: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        content_type: impl Into<String>,
        size_bytes: u64,
        timestamp: u64,
        tags: Vec<String>,
    ) -> Self {
        Self {
            cid: cid.into(),
            title: title.into(),
            description: description.into(),
            content_type: content_type.into(),
            size_bytes,
            timestamp,
            tags,
            schema_version: SCHEMA_VERSION,
        }
    }

    /// Reject structurally invalid envelopes before broadcast/anchor.
    pub fn validate(&self) -> Result<(), EnvelopeError> {
        if self.cid.trim().is_empty() {
            return Err(EnvelopeError::EmptyCid);
        }
        if self.content_type.trim().is_empty() {
            return Err(EnvelopeError::EmptyContentType);
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(EnvelopeError::UnsupportedSchemaVersion(self.schema_version));
        }
        Ok(())
    }

    /// The canonical 32-byte hash of this envelope (see [`crate::metadata_hash`]).
    #[cfg(feature = "hash")]
    pub fn metadata_hash(&self) -> [u8; 32] {
        crate::metadata_hash(self)
    }

    /// The `(cid, metadata_hash)` tuple the batch-anchor tool transports on-chain.
    #[cfg(feature = "hash")]
    pub fn anchor_entry(&self) -> AnchorEntry {
        AnchorEntry {
            cid: self.cid.clone(),
            metadata_hash: self.metadata_hash(),
        }
    }

    /// Serialize to the JSON bytes used as the Delivery payload.
    #[cfg(feature = "json")]
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Parse a Delivery payload back into an envelope.
    #[cfg(feature = "json")]
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}
