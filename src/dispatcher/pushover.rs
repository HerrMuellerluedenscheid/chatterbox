use crate::dispatcher::{DispatchError, Example, Handler};
use crate::message::Message;
use log::error;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Receiver;
use validator::Validate;

/// Push delivery through [Pushover](https://pushover.net).
///
/// Pushover fans a single message out to all of a user's (or group's)
/// registered devices. This dispatcher POSTs a form-encoded request to the
/// [Messages API][api]; you need an application `token` (created in the
/// Pushover dashboard) and the recipient `user` (or group) key.
///
/// [api]: https://pushover.net/api
#[derive(Validate, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Pushover {
    /// Application API token/key, created in the Pushover dashboard.
    pub token: String,

    /// Recipient user key or group key.
    pub user: String,

    /// Optional device name to target a single device instead of all of the
    /// user's devices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

impl Example for Pushover {
    fn example() -> Self {
        Self {
            token: "azGDORePK8gMaC0QOYAMyEEuzJnyUi".to_string(),
            user: "uQiRzpo4DXghDmr9QzzfQu27cmVRsG".to_string(),
            device: None,
        }
    }
}

impl Handler for Pushover {
    fn check(&self) -> Result<(), DispatchError> {
        self.validate().map_err(DispatchError::ValidationError)
    }

    fn start_handler(self, receiver: Receiver<String>) {
        let mut handler = PushoverHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            handler.start().await;
        });
    }
}

#[derive(Serialize)]
struct PushoverPayload<'a> {
    token: &'a str,
    user: &'a str,
    title: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    device: Option<&'a str>,
}

/// Send a message through the Pushover Messages API.
///
/// See <https://pushover.net/api>. Returns the upstream status and body on a
/// non-2xx response so callers can surface a useful error (invalid token or
/// user key, rate limit, …).
pub async fn send_message(
    token: &str,
    user: &str,
    device: Option<&str>,
    message: Message,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let payload = PushoverPayload {
        token,
        user,
        title: &message.title,
        message: &message.body,
        device,
    };

    let response = client
        .post("https://api.pushover.net/1/messages.json")
        .form(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Pushover API error: {} - {}", status, error_text).into());
    }

    Ok(())
}

pub struct PushoverHandler {
    pub(crate) config: Pushover,
    pub(crate) receiver: Receiver<String>,
}

impl PushoverHandler {
    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            if let Err(e) = send_message(
                &self.config.token,
                &self.config.user,
                self.config.device.as_deref(),
                message,
            )
            .await
            {
                error!("failed sending via Pushover: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        Pushover::example();
    }

    #[tokio::test]
    #[ignore = "Requires env vars CHATTERBOX_PUSHOVER_TOKEN and CHATTERBOX_PUSHOVER_USER"]
    async fn test_dispatch_example() {
        let token = std::env::var("CHATTERBOX_PUSHOVER_TOKEN")
            .expect("missing env var CHATTERBOX_PUSHOVER_TOKEN");
        let user = std::env::var("CHATTERBOX_PUSHOVER_USER")
            .expect("missing env var CHATTERBOX_PUSHOVER_USER");

        let test_message = Message::test_example();
        send_message(&token, &user, None, test_message)
            .await
            .unwrap();
    }
}
