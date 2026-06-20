use crate::dispatcher::{DispatchError, Example, Handler};
use crate::message::Message;
use log::error;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Receiver;
use validator::Validate;

/// Discord delivery through an [Incoming Webhook][webhook].
///
/// Unlike [`Discord`](crate::dispatcher::discord::Discord), this dispatcher
/// needs no bot token and no gateway connection: it POSTs straight to a
/// webhook URL of the form
/// `https://discord.com/api/webhooks/{id}/{token}`, so the message is
/// delivered as the integration/app that owns the webhook. The target channel
/// is fixed when the webhook is created in the Discord UI.
///
/// The message is sent as plain `content` (not an embed) so Discord renders the
/// full Markdown set, including headers. See [`render_content`].
///
/// `username` and `avatar_url` are optional per-message overrides Discord
/// applies to the webhook's default identity.
///
/// [webhook]: https://discord.com/developers/docs/resources/webhook#execute-webhook
#[derive(Validate, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct DiscordWebhook {
    #[validate(url)]
    pub webhook_url: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

impl Example for DiscordWebhook {
    fn example() -> Self {
        Self {
            webhook_url: "https://discord.com/api/webhooks/1234567890/EXAMPLE-EXAMPLE_token"
                .to_string(),
            username: Some("Chatterbox".to_string()),
            avatar_url: None,
        }
    }
}

impl Handler for DiscordWebhook {
    fn check(&self) -> Result<(), DispatchError> {
        self.validate().map_err(DispatchError::ValidationError)
    }

    fn start_handler(self, receiver: Receiver<String>) {
        let mut handler = DiscordWebhookHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            handler.start().await;
        });
    }
}

#[derive(Serialize)]
struct WebhookPayload<'a> {
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    avatar_url: Option<&'a str>,
}

/// Render a [`Message`] as Discord message content (Markdown).
///
/// We POST the message as plain webhook `content` rather than an embed: only
/// regular message content renders Discord's full Markdown, including headers
/// (`# `, `## `). Embed `description`/field values support a narrower subset
/// and drop header syntax, printing it literally. The title becomes a level-1
/// header — note the space after `#`, which Discord requires or it treats the
/// `#` as a (failed) channel mention.
fn render_content(message: &Message) -> String {
    match (message.title.is_empty(), message.body.is_empty()) {
        (true, _) => message.body.clone(),
        (false, true) => format!("# {}", message.title),
        (false, false) => format!("# {}\n{}", message.title, message.body),
    }
}

/// Execute a Discord webhook.
///
/// See <https://discord.com/developers/docs/resources/webhook#execute-webhook>.
/// Returns the upstream status and body on a non-2xx response so callers can
/// surface a useful error (revoked/unknown webhook, content too long, …).
pub async fn send_message(
    webhook_url: &str,
    username: Option<&str>,
    avatar_url: Option<&str>,
    message: Message,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let payload = WebhookPayload {
        content: render_content(&message),
        username,
        avatar_url,
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
        return Err(format!("Discord webhook error: {} - {}", status, error_text).into());
    }

    Ok(())
}

pub struct DiscordWebhookHandler {
    pub(crate) config: DiscordWebhook,
    pub(crate) receiver: Receiver<String>,
}

impl DiscordWebhookHandler {
    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            if let Err(e) = send_message(
                &self.config.webhook_url,
                self.config.username.as_deref(),
                self.config.avatar_url.as_deref(),
                message,
            )
            .await
            {
                error!("failed sending via Discord webhook: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        DiscordWebhook::example();
    }

    #[test]
    fn render_content_renders_title_as_header() {
        // Space after `#` is required for Discord to render a header.
        let m = Message::new("Heads up".into(), "the body".into());
        assert_eq!(render_content(&m), "# Heads up\nthe body");
    }

    #[test]
    fn render_content_omits_empty_parts() {
        let title_only = Message::new("Heads up".into(), String::new());
        assert_eq!(render_content(&title_only), "# Heads up");

        let body_only = Message::new(String::new(), "just a body".into());
        assert_eq!(render_content(&body_only), "just a body");
    }

    #[tokio::test]
    #[ignore = "Requires env var CHATTERBOX_DISCORD_WEBHOOK_URL"]
    async fn test_dispatch_example() {
        let webhook_url = std::env::var("CHATTERBOX_DISCORD_WEBHOOK_URL")
            .expect("missing env var CHATTERBOX_DISCORD_WEBHOOK_URL");

        let test_message = Message::test_example();
        send_message(&webhook_url, Some("Chatterbox"), None, test_message)
            .await
            .unwrap();
    }
}
