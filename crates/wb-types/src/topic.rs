//! Logos Delivery (Waku) content-topic conventions for Whistleblower.
//!
//! Waku content topics follow `/<application>/<version>/<topic-name>/<encoding>`.
//! Whistleblower broadcasts document envelopes (JSON) to a single well-known
//! topic so any party — including the permissionless batch-anchor tool — can
//! subscribe and discover newly published documents.

/// Application segment of the content topic.
pub const APP: &str = "whistleblower";
/// Application version segment.
pub const APP_VERSION: &str = "1";
/// Topic-name segment for document-envelope broadcasts.
pub const DOCUMENTS: &str = "documents";
/// Encoding segment. Payloads are JSON-encoded [`crate::MetadataEnvelope`]s.
pub const ENCODING: &str = "json";

/// The default content topic the app publishes document envelopes to:
/// `/whistleblower/1/documents/json`.
pub fn documents_content_topic() -> String {
    build_content_topic(DOCUMENTS)
}

/// Build a Whistleblower content topic for an arbitrary topic-name segment.
pub fn build_content_topic(name: &str) -> String {
    format!("/{APP}/{APP_VERSION}/{name}/{ENCODING}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_topic_is_well_formed() {
        assert_eq!(documents_content_topic(), "/whistleblower/1/documents/json");
    }
}
