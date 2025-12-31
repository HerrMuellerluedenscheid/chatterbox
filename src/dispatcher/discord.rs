use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::broadcast::Receiver;

use crate::dispatcher::{Example, Handler};
use crate::message::Message;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Discord {
    pub bot_token: String,
    pub channel_id: String,
}

impl Example for Discord {
    fn example() -> Self {
        Discord {
            bot_token: "92349823049:DFIPJEXAMPLE-EXAMPLE123d-EXAMPLE".to_string(),
            channel_id: "1234567890".to_string(),
        }
    }
}

impl Handler for Discord {
    fn start_handler(self, receiver: Receiver<String>) {
        let mut handler = DiscordHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            handler.start().await;
        });
    }
}

async fn send_message(
    bot_token: &str,
    channel_id: &str,
    message: Message,
) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!(
            "https://discord.com/api/v10/channels/{}/messages",
            channel_id
        ))
        .header("Authorization", format!("Bot {}", bot_token))
        .header("Content-Type", "application/json")
        .json(&json!({
                "embeds": [{
                    "title": message.title,
                    "description": message.body,
                }]
        }))
        .send()
        .await?;

    response.error_for_status()?;

    Ok(())
}

pub struct DiscordHandler {
    pub(crate) config: Discord,
    pub(crate) receiver: Receiver<String>,
}

impl DiscordHandler {
    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            send_message(&self.config.bot_token, &self.config.channel_id, message)
                .await
                .expect("failed sending message");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std;

    #[test]
    fn test_example() {
        Discord::example();
    }

    #[tokio::test]
    async fn test_dispatch_example() {
        let bot_token = std::env::var("CHATTERBOX_DISCORD_BOT_TOKEN").unwrap();
        let channel_id = std::env::var("CHATTERBOX_DISCORD_CHANNEL_ID").unwrap();
        let test_message = Message::test_example();
        send_message(&bot_token, &channel_id, test_message)
            .await
            .unwrap();
    }
}
