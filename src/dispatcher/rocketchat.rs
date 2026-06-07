use crate::dispatcher::{DispatchError, Example, Handler};
use crate::message::Message;
use log::error;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Receiver;
use validator::Validate;

/// [Rocket.Chat](https://rocket.chat) delivery through an
/// [incoming webhook][webhook].
///
/// Rocket.Chat is a self-hostable team-chat platform whose incoming webhooks
/// accept a Slack-compatible JSON payload. This dispatcher POSTs `{ "text": …
/// }` with the message rendered as markdown (bold title + body); no bot or app
/// is needed, just a webhook URL created in the Rocket.Chat admin UI.
///
/// `channel` and `alias` optionally override the webhook's defaults. Overrides
/// only take effect if the integration has "Script Enabled" / override
/// permissions configured on the server.
///
/// [webhook]: https://docs.rocket.chat/docs/integrations
#[derive(Validate, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct RocketChat {
    #[validate(url)]
    pub webhook_url: String,

    /// Optional channel override (e.g. `#general` or `@username`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,

    /// Optional display name override for the posting integration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

impl Example for RocketChat {
    fn example() -> Self {
        Self {
            webhook_url: "https://rocketchat.example.com/hooks/xxxxxxxxxxxxxxxxx/xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
                .to_string(),
            channel: None,
            alias: Some("Chatterbox".to_string()),
        }
    }
}

impl Handler for RocketChat {
    fn check(&self) -> Result<(), DispatchError> {
        self.validate().map_err(DispatchError::ValidationError)
    }

    fn start_handler(self, receiver: Receiver<String>) {
        let mut handler = RocketChatHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            handler.start().await;
        });
    }
}

#[derive(Serialize)]
struct RocketChatPayload<'a> {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alias: Option<&'a str>,
}

/// Post a message to a Rocket.Chat incoming webhook.
///
/// See <https://docs.rocket.chat/docs/integrations>. Returns the upstream
/// status and body on a non-2xx response so callers can surface a useful error
/// (revoked/unknown webhook, disabled integration, …).
pub async fn send_message(
    webhook_url: &str,
    channel: Option<&str>,
    alias: Option<&str>,
    message: Message,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let payload = RocketChatPayload {
        text: format!("**{}**\n\n{}", message.title, message.body),
        channel,
        alias,
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
        return Err(format!("Rocket.Chat webhook error: {} - {}", status, error_text).into());
    }

    Ok(())
}

pub struct RocketChatHandler {
    pub(crate) config: RocketChat,
    pub(crate) receiver: Receiver<String>,
}

impl RocketChatHandler {
    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            if let Err(e) = send_message(
                &self.config.webhook_url,
                self.config.channel.as_deref(),
                self.config.alias.as_deref(),
                message,
            )
            .await
            {
                error!("failed sending via Rocket.Chat webhook: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        RocketChat::example();
    }

    #[tokio::test]
    #[ignore = "Requires env var CHATTERBOX_ROCKETCHAT_WEBHOOK_URL"]
    async fn test_dispatch_example() {
        let webhook_url = std::env::var("CHATTERBOX_ROCKETCHAT_WEBHOOK_URL")
            .expect("missing env var CHATTERBOX_ROCKETCHAT_WEBHOOK_URL");

        let test_message = Message::test_example();
        send_message(&webhook_url, None, Some("Chatterbox"), test_message)
            .await
            .unwrap();
    }
}
