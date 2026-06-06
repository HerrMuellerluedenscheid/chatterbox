use crate::dispatcher::{DispatchError, Example, Handler};
use crate::message::Message;
use log::error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::broadcast::Receiver;
use validator::Validate;

/// Generic JSON webhook — an escape hatch for any service this crate has no
/// dedicated transport for.
///
/// The full [`Message`] is POSTed as JSON (`{"title","body","subject"}`,
/// exactly the on-the-wire form) to `url`, so it slots straight into automation
/// platforms (Zapier, n8n, Make, IFTTT) or any custom endpoint that can accept
/// a JSON body.
///
/// `headers` are sent verbatim with every request — use it for whatever auth
/// the target expects, e.g. `Authorization: Bearer …` or `X-Api-Key: …`.
#[derive(Validate, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Webhook {
    #[validate(url)]
    pub url: String,

    /// Extra HTTP headers sent with every request (typically for auth).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
}

impl Example for Webhook {
    fn example() -> Self {
        let mut headers = HashMap::new();
        headers.insert(
            "Authorization".to_string(),
            "Bearer xxxxxxxxxxxxxxxx".to_string(),
        );
        Self {
            url: "https://example.com/hooks/chatterbox".to_string(),
            headers,
        }
    }
}

impl Handler for Webhook {
    fn check(&self) -> Result<(), DispatchError> {
        self.validate().map_err(DispatchError::ValidationError)
    }

    fn start_handler(self, receiver: Receiver<String>) {
        let mut handler = WebhookHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            handler.start().await;
        });
    }
}

/// POST a message to a generic JSON webhook.
///
/// The body is the serialized [`Message`]. Returns the upstream status and body
/// on a non-2xx response so callers can surface a useful error.
pub async fn send_message(
    url: &str,
    headers: &HashMap<String, String>,
    message: Message,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let mut request = client.post(url).json(&message);
    for (key, value) in headers {
        request = request.header(key, value);
    }

    let response = request.send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Webhook error: {} - {}", status, error_text).into());
    }

    Ok(())
}

pub struct WebhookHandler {
    pub(crate) config: Webhook,
    pub(crate) receiver: Receiver<String>,
}

impl WebhookHandler {
    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            if let Err(e) = send_message(&self.config.url, &self.config.headers, message).await {
                error!("failed sending via webhook: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        Webhook::example();
    }

    #[tokio::test]
    #[ignore = "Requires env var CHATTERBOX_WEBHOOK_URL"]
    async fn test_dispatch_example() {
        let url = std::env::var("CHATTERBOX_WEBHOOK_URL")
            .expect("missing env var CHATTERBOX_WEBHOOK_URL");

        let test_message = Message::test_example();
        send_message(&url, &HashMap::new(), test_message)
            .await
            .unwrap();
    }
}
