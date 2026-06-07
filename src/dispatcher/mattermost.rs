use crate::dispatcher::{DispatchError, Example, Handler};
use crate::message::Message;
use log::error;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Receiver;
use validator::Validate;

/// [Mattermost](https://mattermost.com) delivery through an
/// [incoming webhook][webhook].
///
/// Mattermost is a self-hostable Slack alternative, and its incoming webhooks
/// accept a Slack-compatible JSON payload. This dispatcher POSTs `{ "text": …
/// }` with the message rendered as markdown (bold title + body), so it needs no
/// bot and no app — just a webhook URL created in the Mattermost UI.
///
/// `channel` and `username` optionally override the webhook's defaults; a
/// `channel` override only works if the webhook was created with
/// "allow channel override" enabled.
///
/// [webhook]: https://developers.mattermost.com/integrate/webhooks/incoming/
#[derive(Validate, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Mattermost {
    #[validate(url)]
    pub webhook_url: String,

    /// Optional channel override (e.g. `town-square` or `@username`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,

    /// Optional display name override for the posting integration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

impl Example for Mattermost {
    fn example() -> Self {
        Self {
            webhook_url: "https://mattermost.example.com/hooks/xxxxxxxxxxxxxxxxxxxxxxxxxx"
                .to_string(),
            channel: None,
            username: Some("Chatterbox".to_string()),
        }
    }
}

impl Handler for Mattermost {
    fn check(&self) -> Result<(), DispatchError> {
        self.validate().map_err(DispatchError::ValidationError)
    }

    fn start_handler(self, receiver: Receiver<String>) {
        let mut handler = MattermostHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            handler.start().await;
        });
    }
}

#[derive(Serialize)]
struct MattermostPayload<'a> {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<&'a str>,
}

/// Post a message to a Mattermost incoming webhook.
///
/// See <https://developers.mattermost.com/integrate/webhooks/incoming/>.
/// Returns the upstream status and body on a non-2xx response so callers can
/// surface a useful error (revoked/unknown webhook, channel override not
/// allowed, …).
pub async fn send_message(
    webhook_url: &str,
    channel: Option<&str>,
    username: Option<&str>,
    message: Message,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let payload = MattermostPayload {
        text: format!("**{}**\n\n{}", message.title, message.body),
        channel,
        username,
    };

    let response = client
        .post(webhook_url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Mattermost webhook error: {} - {}", status, error_text).into());
    }

    Ok(())
}

pub struct MattermostHandler {
    pub(crate) config: Mattermost,
    pub(crate) receiver: Receiver<String>,
}

impl MattermostHandler {
    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            if let Err(e) = send_message(
                &self.config.webhook_url,
                self.config.channel.as_deref(),
                self.config.username.as_deref(),
                message,
            )
            .await
            {
                error!("failed sending via Mattermost webhook: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        Mattermost::example();
    }

    #[tokio::test]
    #[ignore = "Requires env var CHATTERBOX_MATTERMOST_WEBHOOK_URL"]
    async fn test_dispatch_example() {
        let webhook_url = std::env::var("CHATTERBOX_MATTERMOST_WEBHOOK_URL")
            .expect("missing env var CHATTERBOX_MATTERMOST_WEBHOOK_URL");

        let test_message = Message::test_example();
        send_message(&webhook_url, None, Some("Chatterbox"), test_message)
            .await
            .unwrap();
    }
}
