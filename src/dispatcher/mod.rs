pub mod discord;
pub mod discord_webhook;
pub mod email;
pub mod google_chat;
pub mod gotify;
pub mod matrix;
pub mod mattermost;
pub mod ntfy;
pub mod pushover;
pub mod resend;
pub mod rocketchat;
pub mod slack;
pub mod teams;
pub mod telegram;
pub mod webhook;

use discord::Discord;
use discord_webhook::DiscordWebhook;
use email::Email;
use google_chat::GoogleChat;
use gotify::Gotify;
use matrix::Matrix;
use mattermost::Mattermost;
use ntfy::Ntfy;
use pushover::Pushover;
use resend::Resend;
use rocketchat::RocketChat;
use slack::Slack;
use teams::Teams;
use telegram::Telegram;
use webhook::Webhook;

use log::debug;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::broadcast;
use tokio::sync::broadcast::Receiver;
use validator::ValidationErrors;

#[derive(Error, Debug)]
pub enum DispatchError {
    #[error("Dispatcher test failed {:?}", .0)]
    Check(String),
    #[error("Validation failed {:?}", .0)]
    ValidationError(ValidationErrors),
}

/// Bind a handler to a receiver channel if the handler is not `None`.
macro_rules! setup_handler {
    ($handler:expr, $receiver:expr) => {{
        if let Some(config) = $handler {
            config.check()?;
            let rx = $receiver.subscribe();
            config.start_handler(rx);
            debug!("started handler");
        }
    }};
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Default, Clone)]
pub struct Sender {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram: Option<Telegram>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<Email>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub slack: Option<Slack>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord: Option<Discord>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord_webhook: Option<DiscordWebhook>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub teams: Option<Teams>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub gotify: Option<Gotify>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub resend: Option<Resend>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ntfy: Option<Ntfy>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pushover: Option<Pushover>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub matrix: Option<Matrix>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub mattermost: Option<Mattermost>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub rocketchat: Option<RocketChat>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_chat: Option<GoogleChat>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook: Option<Webhook>,
}

impl Example for Sender {
    fn example() -> Self {
        Self {
            telegram: Some(Telegram::example()),
            email: Some(Email::example()),
            slack: Some(Slack::example()),
            discord: Some(Discord::example()),
            discord_webhook: Some(DiscordWebhook::example()),
            teams: Some(Teams::example()),
            gotify: Some(Gotify::example()),
            resend: Some(Resend::example()),
            ntfy: Some(Ntfy::example()),
            pushover: Some(Pushover::example()),
            matrix: Some(Matrix::example()),
            mattermost: Some(Mattermost::example()),
            rocketchat: Some(RocketChat::example()),
            google_chat: Some(GoogleChat::example()),
            webhook: Some(Webhook::example()),
        }
    }
}

impl Sender {
    pub fn setup_dispatcher(self, tx: &broadcast::Sender<String>) -> Result<(), DispatchError> {
        setup_handler!(self.telegram, tx);
        setup_handler!(self.email, tx);
        setup_handler!(self.slack, tx);
        setup_handler!(self.discord, tx);
        setup_handler!(self.discord_webhook, tx);
        setup_handler!(self.teams, tx);
        setup_handler!(self.gotify, tx);
        setup_handler!(self.resend, tx);
        setup_handler!(self.ntfy, tx);
        setup_handler!(self.pushover, tx);
        setup_handler!(self.matrix, tx);
        setup_handler!(self.mattermost, tx);
        setup_handler!(self.rocketchat, tx);
        setup_handler!(self.google_chat, tx);
        setup_handler!(self.webhook, tx);
        Ok(())
    }
}

trait Handler {
    fn check(&self) -> Result<(), DispatchError> {
        Ok(())
    }
    fn start_handler(self, receiver: Receiver<String>);
}

pub trait Example {
    fn example() -> Self;
}

#[test]
fn test_default_config() {
    println!("{:?}", Sender::example());
}
