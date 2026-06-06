use crate::dispatcher::{DispatchError, Example, Handler};
use crate::message::Message;
use log::error;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::sync::broadcast::Receiver;
use validator::Validate;

/// Push delivery through [ntfy](https://ntfy.sh) (the public service or a
/// self-hosted server).
///
/// Like [`Gotify`](crate::dispatcher::gotify::Gotify), ntfy is a simple
/// pub/sub push server: a message is published to a *topic* and every
/// subscriber to that topic receives it. This dispatcher publishes via the
/// [JSON endpoint][publish] (`POST {server_url}` with the topic in the body),
/// which carries the title and body as UTF-8 without the header-encoding
/// limitations of the path-based API.
///
/// `access_token` is only needed for protected topics; on the public
/// `ntfy.sh` server reading and writing open topics needs no auth.
///
/// [publish]: https://docs.ntfy.sh/publish/#publish-as-json
#[derive(Validate, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Ntfy {
    /// Base URL of the ntfy server, e.g. `https://ntfy.sh`.
    pub server_url: Url,

    /// Topic to publish to. Anyone who knows the topic of an unprotected
    /// server can read it, so treat it like a (weak) secret.
    pub topic: String,

    /// Optional access token (`tk_...`) for reserved/protected topics, sent as
    /// `Authorization: Bearer <token>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
}

impl Example for Ntfy {
    fn example() -> Self {
        Self {
            server_url: Url::from_str("https://ntfy.sh").unwrap(),
            topic: "chatterbox-alerts".to_string(),
            access_token: None,
        }
    }
}

impl Handler for Ntfy {
    fn check(&self) -> Result<(), DispatchError> {
        self.validate().map_err(DispatchError::ValidationError)
    }

    fn start_handler(self, receiver: Receiver<String>) {
        let mut handler = NtfyHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            handler.start().await;
        });
    }
}

#[derive(Serialize)]
struct NtfyPayload<'a> {
    topic: &'a str,
    title: &'a str,
    message: &'a str,
}

/// Publish a message to an ntfy topic.
///
/// See <https://docs.ntfy.sh/publish/#publish-as-json>. Returns the upstream
/// status and body on a non-2xx response so callers can surface a useful error
/// (unknown topic permissions, invalid token, …).
pub async fn send_message(
    server_url: &str,
    topic: &str,
    access_token: Option<&str>,
    message: Message,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let payload = NtfyPayload {
        topic,
        title: &message.title,
        message: &message.body,
    };

    let mut request = client.post(server_url).json(&payload);
    if let Some(token) = access_token {
        request = request.bearer_auth(token);
    }

    let response = request.send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("ntfy API error: {} - {}", status, error_text).into());
    }

    Ok(())
}

pub struct NtfyHandler {
    pub(crate) config: Ntfy,
    pub(crate) receiver: Receiver<String>,
}

impl NtfyHandler {
    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            if let Err(e) = send_message(
                self.config.server_url.as_ref(),
                &self.config.topic,
                self.config.access_token.as_deref(),
                message,
            )
            .await
            {
                error!("failed sending to ntfy: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        Ntfy::example();
    }

    #[tokio::test]
    #[ignore = "Requires env vars CHATTERBOX_NTFY_SERVER_URL and CHATTERBOX_NTFY_TOPIC (optionally CHATTERBOX_NTFY_ACCESS_TOKEN)"]
    async fn test_dispatch_example() {
        let server_url = std::env::var("CHATTERBOX_NTFY_SERVER_URL")
            .expect("missing env var CHATTERBOX_NTFY_SERVER_URL");
        let topic =
            std::env::var("CHATTERBOX_NTFY_TOPIC").expect("missing env var CHATTERBOX_NTFY_TOPIC");
        let access_token = std::env::var("CHATTERBOX_NTFY_ACCESS_TOKEN").ok();

        let test_message = Message::test_example();
        send_message(&server_url, &topic, access_token.as_deref(), test_message)
            .await
            .unwrap();
    }
}
