use crate::dispatcher::{DispatchError, Example, Handler};
use crate::message::Message;
use serde::{Deserialize, Serialize};
use slack_hook::{PayloadBuilder, Slack as SlackHook};
use tokio::sync::broadcast::Receiver;
use validator::Validate;

#[derive(Validate, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Slack {
    #[serde(default)]
    #[validate(url)]
    pub webhook_url: String,

    #[serde(default)]
    pub channel: String,
}

impl Example for Slack {
    fn example() -> Self {
        Self {
            webhook_url: "https://hooks.slack.com/services/XXXXXXXXX/XXXXXXXXXXX".to_string(),
            channel: "#some-channel".to_string(),
        }
    }
}

impl Handler for Slack {
    fn check(&self) -> Result<(), DispatchError> {
        self.validate().map_err(DispatchError::ValidationError)
    }

    fn start_handler(self, receiver: Receiver<String>) {
        let mut handler = SlackHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            handler.start().await;
        });
    }
}

/// Send messages to slack webhook
pub async fn send_message(
    webhook_url: &str,
    channel: &str,
    message: Message,
) -> Result<(), slack_hook::Error> {
    let slack = SlackHook::new(webhook_url)?;
    let p = PayloadBuilder::new()
        .text(message.markdown())
        .channel(channel)
        .username("Chatterbox")
        .build()?;

    slack.send(&p).await
}

pub struct SlackHandler {
    pub(crate) config: Slack,
    pub(crate) receiver: Receiver<String>,
}

impl SlackHandler {
    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            send_message(&self.config.webhook_url, &self.config.channel, message)
                .await
                .expect("failed sending on slack");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        Slack::example();
    }

    #[tokio::test]
    async fn test_dispatch_example() {
        use std;

        let webhook_url = std::env::var("CHATTERBOX_SLACK_WEBHOOK_URL")
            .expect("missing env var CHATTERBOX_SLACK_WEBHOOK_URL");
        let channel = std::env::var("CHATTERBOX_SLACK_CHANNEL")
            .expect("missing env var CHATTERBOX_SLACK_CHANNEL");

        let test_message = Message::test_example();
        send_message(&webhook_url, &channel, test_message)
            .await
            .unwrap();
    }
}
