//! Logos Delivery (Waku) client.
//!
//! Uses the autosharding relay REST API so callers only deal in content topics
//! (`/whistleblower/1/documents/json`) and the node maps them to shards:
//!
//! * subscribe:   `POST /relay/v1/auto/subscriptions`        body `["<topic>"]`
//! * unsubscribe: `DELETE /relay/v1/auto/subscriptions`      body `["<topic>"]`
//! * publish:     `POST /relay/v1/auto/messages/{topic}`     body `RelayWakuMessage`
//! * poll/drain:  `GET  /relay/v1/auto/messages/{topic}`     -> `[RelayWakuMessage]`
//!
//! Message payloads are base64-encoded. `GET` drains messages buffered since the
//! last poll, which is exactly the accumulation primitive the batch runner needs.

use base64::Engine;
use serde::Deserialize;

use crate::error::DeliveryError;

/// A message received from a Delivery topic, payload already base64-decoded.
#[derive(Clone, Debug)]
pub struct DeliveryMessage {
    pub content_topic: String,
    pub payload: Vec<u8>,
    /// Waku timestamp in nanoseconds (0 if the node did not set one).
    pub timestamp_ns: u64,
    pub message_hash: Option<String>,
}

/// Publish/subscribe over Logos Delivery.
pub trait DeliveryClient {
    async fn subscribe(&self, content_topic: &str) -> Result<(), DeliveryError>;
    async fn unsubscribe(&self, content_topic: &str) -> Result<(), DeliveryError>;
    /// Publish a raw payload to a content topic.
    async fn publish(
        &self,
        content_topic: &str,
        payload: &[u8],
        timestamp_ns: Option<u64>,
    ) -> Result<(), DeliveryError>;
    /// Drain messages buffered on a content topic since the previous poll.
    async fn poll(&self, content_topic: &str) -> Result<Vec<DeliveryMessage>, DeliveryError>;
}

/// HTTP implementation against an nwaku-style Delivery node.
#[derive(Clone, Debug)]
pub struct HttpDelivery {
    client: reqwest::Client,
    base_url: String,
}

impl HttpDelivery {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    /// Default local node REST endpoint: `http://127.0.0.1:8645`.
    pub fn local() -> Self {
        Self::new("http://127.0.0.1:8645")
    }

    pub fn with_client(client: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    async fn subscription_call(
        &self,
        method: reqwest::Method,
        content_topic: &str,
    ) -> Result<(), DeliveryError> {
        let url = format!("{}/relay/v1/auto/subscriptions", self.base_url);
        let resp = self
            .client
            .request(method, &url)
            .json(&[content_topic])
            .send()
            .await?;
        ok_or_status(resp).await.map(|_| ())
    }
}

/// Percent-encode a content topic for use in a URL path (encodes `/` etc.).
fn encode_topic(topic: &str) -> String {
    let mut out = String::with_capacity(topic.len() * 3);
    for b in topic.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

async fn ok_or_status(resp: reqwest::Response) -> Result<reqwest::Response, DeliveryError> {
    let status = resp.status();
    if status.is_success() {
        Ok(resp)
    } else {
        let body = resp.text().await.unwrap_or_default();
        Err(DeliveryError::Status {
            status: status.as_u16(),
            body,
        })
    }
}

/// Wire shape of a relay message in either direction.
#[derive(Deserialize)]
struct WireMessage {
    payload: String,
    #[serde(rename = "contentTopic", default)]
    content_topic: String,
    #[serde(default)]
    timestamp: u64,
    #[serde(rename = "messageHash", default)]
    message_hash: Option<String>,
}

impl DeliveryClient for HttpDelivery {
    async fn subscribe(&self, content_topic: &str) -> Result<(), DeliveryError> {
        self.subscription_call(reqwest::Method::POST, content_topic)
            .await
    }

    async fn unsubscribe(&self, content_topic: &str) -> Result<(), DeliveryError> {
        self.subscription_call(reqwest::Method::DELETE, content_topic)
            .await
    }

    async fn publish(
        &self,
        content_topic: &str,
        payload: &[u8],
        timestamp_ns: Option<u64>,
    ) -> Result<(), DeliveryError> {
        let url = format!(
            "{}/relay/v1/auto/messages/{}",
            self.base_url,
            encode_topic(content_topic)
        );
        let body = serde_json::json!({
            "contentTopic": content_topic,
            "payload": base64::engine::general_purpose::STANDARD.encode(payload),
            "timestamp": timestamp_ns.unwrap_or(0),
            "ephemeral": false,
        });
        let resp = self.client.post(&url).json(&body).send().await?;
        ok_or_status(resp).await.map(|_| ())
    }

    async fn poll(&self, content_topic: &str) -> Result<Vec<DeliveryMessage>, DeliveryError> {
        let url = format!(
            "{}/relay/v1/auto/messages/{}",
            self.base_url,
            encode_topic(content_topic)
        );
        let resp = ok_or_status(self.client.get(&url).send().await?).await?;
        let wire: Vec<WireMessage> = resp.json().await?;

        let mut out = Vec::with_capacity(wire.len());
        for m in wire {
            let payload = base64::engine::general_purpose::STANDARD
                .decode(m.payload.as_bytes())
                .map_err(|e| DeliveryError::Payload(format!("base64: {e}")))?;
            out.push(DeliveryMessage {
                content_topic: if m.content_topic.is_empty() {
                    content_topic.to_string()
                } else {
                    m.content_topic
                },
                payload,
                timestamp_ns: m.timestamp,
                message_hash: m.message_hash,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_is_percent_encoded() {
        assert_eq!(
            encode_topic("/whistleblower/1/documents/json"),
            "%2Fwhistleblower%2F1%2Fdocuments%2Fjson"
        );
    }
}
