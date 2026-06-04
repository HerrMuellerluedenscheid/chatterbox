use crate::dispatcher::{DispatchError, Example, Handler};
use crate::message::Message;
use log::error;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Receiver;
use validator::Validate;

/// Microsoft Teams delivery through an [Incoming Webhook][webhook].
///
/// Like [`DiscordWebhook`](crate::dispatcher::discord_webhook::DiscordWebhook),
/// this dispatcher needs no bot and no app registration: it POSTs straight to
/// a webhook URL created in Teams. The target channel is fixed when the
/// webhook is created.
///
/// The payload is an [Adaptive Card][card] wrapped in the `message` envelope
/// expected by the current Teams "Workflows" (Power Automate) webhooks. The
/// retired Office 365 connector webhooks (`*.webhook.office.com`) accept the
/// same envelope, so both flavours are supported.
///
/// [webhook]: https://learn.microsoft.com/en-us/microsoftteams/platform/concepts/build-and-test/webhooks-and-connectors/how-to/add-incoming-webhook
/// [card]: https://adaptivecards.io/
#[derive(Validate, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Teams {
    #[validate(url)]
    pub webhook_url: String,
}

impl Example for Teams {
    fn example() -> Self {
        Self {
            webhook_url: "https://example.webhook.office.com/webhookb2/00000000-0000-0000-0000-000000000000@00000000-0000-0000-0000-000000000000/IncomingWebhook/0000000000000000000000000000000/00000000-0000-0000-0000-000000000000"
                .to_string(),
        }
    }
}

impl Handler for Teams {
    fn check(&self) -> Result<(), DispatchError> {
        self.validate().map_err(DispatchError::ValidationError)
    }

    fn start_handler(self, receiver: Receiver<String>) {
        let mut handler = TeamsHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            handler.start().await;
        });
    }
}

#[derive(Serialize)]
struct TextBlock<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    text: &'a str,
    wrap: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    weight: Option<&'a str>,
}

#[derive(Serialize)]
struct AdaptiveCard<'a> {
    #[serde(rename = "$schema")]
    schema: &'a str,
    #[serde(rename = "type")]
    kind: &'a str,
    version: &'a str,
    body: [TextBlock<'a>; 2],
}

#[derive(Serialize)]
struct Attachment<'a> {
    #[serde(rename = "contentType")]
    content_type: &'a str,
    content: AdaptiveCard<'a>,
}

#[derive(Serialize)]
struct TeamsPayload<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    attachments: [Attachment<'a>; 1],
}

/// Post an Adaptive Card to a Teams incoming webhook.
///
/// Returns the upstream status and body on a non-2xx response so callers can
/// surface a useful error (revoked/unknown webhook, malformed card, …).
pub async fn send_message(
    webhook_url: &str,
    message: Message,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let payload = TeamsPayload {
        kind: "message",
        attachments: [Attachment {
            content_type: "application/vnd.microsoft.card.adaptive",
            content: AdaptiveCard {
                schema: "http://adaptivecards.io/schemas/adaptive-card.json",
                kind: "AdaptiveCard",
                version: "1.5",
                body: [
                    TextBlock {
                        kind: "TextBlock",
                        text: &message.title,
                        wrap: true,
                        size: Some("Large"),
                        weight: Some("Bolder"),
                    },
                    TextBlock {
                        kind: "TextBlock",
                        text: &message.body,
                        wrap: true,
                        size: None,
                        weight: None,
                    },
                ],
            },
        }],
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
        return Err(format!("Teams webhook error: {} - {}", status, error_text).into());
    }

    Ok(())
}

pub struct TeamsHandler {
    pub(crate) config: Teams,
    pub(crate) receiver: Receiver<String>,
}

impl TeamsHandler {
    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            if let Err(e) = send_message(&self.config.webhook_url, message).await {
                error!("failed sending via Teams webhook: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        Teams::example();
    }

    #[tokio::test]
    #[ignore = "Requires env var CHATTERBOX_TEAMS_WEBHOOK_URL"]
    async fn test_dispatch_example() {
        let webhook_url = std::env::var("CHATTERBOX_TEAMS_WEBHOOK_URL")
            .expect("missing env var CHATTERBOX_TEAMS_WEBHOOK_URL");

        let test_message = Message::test_example();
        send_message(&webhook_url, test_message).await.unwrap();
    }
}
