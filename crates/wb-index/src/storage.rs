//! Logos Storage (Codex) client.
//!
//! The Storage node exposes a REST API; uploading raw bytes returns a plain-text
//! CID. Note the base-path split observed across the ecosystem: a current
//! `logos-storage-nim` node serves `/api/storage/v1`, while the published
//! JS/Python SDKs still target `/api/codex/v1`. [`HttpStorage::new`] takes the
//! full base URL so you can match whichever node you run.

use wb_types::Cid;

use crate::error::StorageError;

/// Store and retrieve bytes on Logos Storage.
pub trait StorageClient {
    /// Upload bytes, returning the resulting CID.
    async fn upload(
        &self,
        bytes: &[u8],
        content_type: Option<&str>,
        filename: Option<&str>,
    ) -> Result<Cid, StorageError>;

    /// Download bytes for a CID (fetching from the network if not held locally).
    async fn download(&self, cid: &str) -> Result<Vec<u8>, StorageError>;
}

/// HTTP implementation against a Codex-style Storage node.
#[derive(Clone, Debug)]
pub struct HttpStorage {
    client: reqwest::Client,
    /// Full base URL including the API prefix, e.g.
    /// `http://localhost:8080/api/storage/v1`.
    base_url: String,
}

impl HttpStorage {
    /// Build a client against an explicit base URL (including the API prefix).
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    /// Default local node: `http://localhost:8080/api/storage/v1`.
    pub fn local() -> Self {
        Self::new("http://localhost:8080/api/storage/v1")
    }

    /// Use a caller-provided `reqwest::Client` (custom timeouts, proxies, etc.).
    pub fn with_client(client: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }
}

impl StorageClient for HttpStorage {
    async fn upload(
        &self,
        bytes: &[u8],
        content_type: Option<&str>,
        filename: Option<&str>,
    ) -> Result<Cid, StorageError> {
        use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE};

        let url = format!("{}/data", self.base_url);
        let mut req = self
            .client
            .post(&url)
            .header(
                CONTENT_TYPE,
                content_type.unwrap_or("application/octet-stream"),
            )
            .body(bytes.to_vec());
        if let Some(name) = filename {
            // Sanitize quotes to keep the header well-formed.
            let safe = name.replace('"', "");
            req = req.header(
                CONTENT_DISPOSITION,
                format!("attachment; filename=\"{safe}\""),
            );
        }

        let resp = req.send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(StorageError::Status {
                status: status.as_u16(),
                body: text,
            });
        }
        let cid = text.trim().to_string();
        if cid.is_empty() {
            return Err(StorageError::EmptyCid);
        }
        Ok(cid)
    }

    async fn download(&self, cid: &str) -> Result<Vec<u8>, StorageError> {
        // `/network/stream` fetches from the network if the node lacks it locally.
        let url = format!("{}/data/{}/network/stream", self.base_url, cid);
        let resp = self.client.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(StorageError::Status {
                status: status.as_u16(),
                body,
            });
        }
        Ok(resp.bytes().await?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_trailing_slash_is_normalized() {
        let s = HttpStorage::new("http://localhost:8080/api/storage/v1/");
        assert_eq!(s.base_url, "http://localhost:8080/api/storage/v1");
    }
}
