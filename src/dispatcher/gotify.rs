use std::str::FromStr;
use reqwest::Url;
use crate::dispatcher::{DispatchError, Example, Handler};
use crate::message::Message;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Receiver;
use validator::Validate;

#[derive(Validate, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Gotify {
    pub server_url: Url,

    #[serde(default)]
    pub app_token: String,
}

impl Example for Gotify {
    fn example() -> Self {
        Self {
            server_url: Url::from_str("https://gotify.example.com").unwrap(),
            app_token: "AxxxxxxxxxxxxxxxxxX".to_string(),
        }
    }
}

impl Handler for Gotify {
    fn check(&self) -> Result<(), DispatchError> {
        self.validate().map_err(DispatchError::ValidationError)
    }

    fn start_handler(self, receiver: Receiver<String>) {
        let mut handler = GotifyHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            handler.start().await;
        });
    }
}

#[derive(Serialize)]
struct GotifyPayload {
    title: String,
    message: String,
    priority: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    extras: Option<serde_json::Value>,
}

/// Send messages to Gotify server
pub async fn send_message(
    server_url: &str,
    app_token: &str,
    message: Message,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let url = format!("{}/message", server_url.trim_end_matches('/'));

    let payload = GotifyPayload {
        title: message.title.clone(),
        message: message.body.clone(),
        priority: 5,
        extras: None,
    };

    let response = client
        .post(&url)
        .header("X-Gotify-Key", app_token)
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Gotify API error: {} - {}", status, error_text).into());
    }

    Ok(())
}

pub struct GotifyHandler {
    pub(crate) config: Gotify,
    pub(crate) receiver: Receiver<String>,
}

impl GotifyHandler {
    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            send_message((&self.config.server_url).as_ref(), &self.config.app_token, message)
                .await
                .expect("failed sending to Gotify");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        Gotify::example();
    }

    #[tokio::test]
    #[ignore = "Requires env vars CHATTERBOX_GOTIFY_SERVER_URL and CHATTERBOX_GOTIFY_APP_TOKEN"]
    async fn test_dispatch_example() {
        use std;

        let server_url = std::env::var("CHATTERBOX_GOTIFY_SERVER_URL")
            .expect("missing env var CHATTERBOX_GOTIFY_SERVER_URL");
        let app_token = std::env::var("CHATTERBOX_GOTIFY_APP_TOKEN")
            .expect("missing env var CHATTERBOX_GOTIFY_APP_TOKEN");

        let test_message = Message::test_example();
        send_message(&server_url, &app_token, test_message)
            .await
            .unwrap();
    }
}