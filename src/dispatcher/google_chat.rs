use crate::dispatcher::{DispatchError, Example, Handler};
use crate::message::Message;
use log::error;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Receiver;
use validator::Validate;

/// [Google Chat](https://chat.google.com) delivery through an
/// [incoming webhook][webhook].
///
/// A Google Workspace space can expose a webhook URL (containing the space id
/// plus a `key`/`token` query pair) that accepts a simple `{ "text": … }` JSON
/// body. This dispatcher posts the message with Google Chat's lightweight
/// formatting — a `*bold*` title followed by the body — so it needs no bot and
/// no app, just the webhook URL created in the space settings.
///
/// [webhook]: https://developers.google.com/workspace/chat/quickstart/webhooks
#[derive(Validate, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct GoogleChat {
    #[validate(url)]
    pub webhook_url: String,
}

impl Example for GoogleChat {
    fn example() -> Self {
        Self {
            webhook_url:
                "https://chat.googleapis.com/v1/spaces/AAAAxxxxxxx/messages?key=XXXX&token=YYYY"
                    .to_string(),
        }
    }
}

impl Handler for GoogleChat {
    fn check(&self) -> Result<(), DispatchError> {
        self.validate().map_err(DispatchError::ValidationError)
    }

    fn start_handler(self, receiver: Receiver<String>) {
        let mut handler = GoogleChatHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            handler.start().await;
        });
    }
}

#[derive(Serialize)]
struct GoogleChatPayload {
    text: String,
}

/// Post a message to a Google Chat incoming webhook.
///
/// See <https://developers.google.com/workspace/chat/quickstart/webhooks>.
/// Google Chat uses single asterisks for bold, so the title is wrapped in `*`.
/// Returns the upstream status and body on a non-2xx response so callers can
/// surface a useful error (revoked webhook, invalid key/token, …).
pub async fn send_message(
    webhook_url: &str,
    message: Message,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let payload = GoogleChatPayload {
        text: format!("*{}*\n\n{}", message.title, message.body),
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
        return Err(format!("Google Chat webhook error: {} - {}", status, error_text).into());
    }

    Ok(())
}

pub struct GoogleChatHandler {
    pub(crate) config: GoogleChat,
    pub(crate) receiver: Receiver<String>,
}

impl GoogleChatHandler {
    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            if let Err(e) = send_message(&self.config.webhook_url, message).await {
                error!("failed sending via Google Chat webhook: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        GoogleChat::example();
    }

    #[tokio::test]
    #[ignore = "Requires env var CHATTERBOX_GOOGLE_CHAT_WEBHOOK_URL"]
    async fn test_dispatch_example() {
        let webhook_url = std::env::var("CHATTERBOX_GOOGLE_CHAT_WEBHOOK_URL")
            .expect("missing env var CHATTERBOX_GOOGLE_CHAT_WEBHOOK_URL");

        let test_message = Message::test_example();
        send_message(&webhook_url, test_message).await.unwrap();
    }
}
