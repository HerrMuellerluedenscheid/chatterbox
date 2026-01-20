use crate::dispatcher::{DispatchError, Example, Handler};
use crate::message::Message;
use lettre::message::{header::ContentType, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::Message as LettreMessage;
use lettre::{SmtpTransport, Transport};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Receiver;
use validator::Validate;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Validate)]
pub struct Email {
    pub smtp_user: String,
    pub smtp_password: String,
    pub smtp_server: String,

    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,

    #[validate(email)]
    pub receiver_address: String,

    #[serde(default = "default_sender_address")]
    #[validate(email)]
    pub sender_address: String,

    #[serde(default = "default_sender_name")]
    pub sender_name: String,
}

fn default_smtp_port() -> u16 {
    587
}

fn default_sender_address() -> String {
    "noreply@chatterbox.local".to_string()
}

fn default_sender_name() -> String {
    "Chatterbox".to_string()
}

impl Example for Email {
    fn example() -> Self {
        Self {
            smtp_user: "USERNAME".to_string(),
            smtp_password: "SUPERSECUREPASSWORD".to_string(),
            smtp_server: "smtp.example.com".to_string(),
            smtp_port: 587,
            receiver_address: "foo.bar@example.com".to_string(),
            sender_address: "noreply@chatterbox.local".to_string(),
            sender_name: "Chatterbox".to_string(),
        }
    }
}

impl Handler for Email {
    fn check(&self) -> Result<(), DispatchError> {
        self.validate().map_err(DispatchError::ValidationError)
    }

    fn start_handler(self, receiver: Receiver<String>) {
        let mut handler = EmailHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            handler.start().await;
        });
    }
}

/// Send email message via SMTP
#[allow(clippy::too_many_arguments)]
pub async fn send_message(
    smtp_server: &str,
    smtp_user: &str,
    smtp_password: &str,
    sender_address: &str,
    sender_name: &str,
    receiver_address: &str,
    message: Message,
) -> Result<(), Box<dyn std::error::Error>> {
    let from = format!("{}<{}>", sender_name, sender_address);

    let html_part = SinglePart::builder()
        .header(ContentType::TEXT_HTML)
        .body(message.html());

    let plain_part = SinglePart::builder()
        .header(ContentType::TEXT_PLAIN)
        .body(message.markdown());

    let email = LettreMessage::builder()
        .from(from.parse()?)
        .reply_to(sender_address.parse()?)
        .to(receiver_address.parse().unwrap())
        .subject(message.title.clone())
        .multipart(
            MultiPart::alternative()
                .singlepart(plain_part)
                .singlepart(html_part),
        )
        .unwrap();

    let credentials = Credentials::new(smtp_user.to_string(), smtp_password.to_string());

    let mailer = SmtpTransport::starttls_relay(smtp_server)
        .unwrap()
        .credentials(credentials)
        .build();
    mailer.send(&email)?;

    Ok(())
}

pub struct EmailHandler {
    pub(crate) config: Email,
    pub(crate) receiver: Receiver<String>,
}

impl EmailHandler {
    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            send_message(
                &self.config.smtp_server,
                &self.config.smtp_user,
                &self.config.smtp_password,
                &self.config.sender_address,
                &self.config.sender_name,
                &self.config.receiver_address,
                message,
            )
            .await
            .expect("failed sending email");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        Email::example();
    }

    #[tokio::test]
    async fn test_dispatch_example() {
        use std;

        let smtp_server = std::env::var("CHATTERBOX_SMTP_SERVER")
            .expect("missing env var CHATTERBOX_SMTP_SERVER");
        let smtp_user =
            std::env::var("CHATTERBOX_SMTP_USER").expect("missing env var CHATTERBOX_SMTP_USER");
        let smtp_password = std::env::var("CHATTERBOX_SMTP_PASSWORD")
            .expect("missing env var CHATTERBOX_SMTP_PASSWORD");
        let receiver_address = std::env::var("CHATTERBOX_EMAIL_RECEIVER")
            .expect("missing env var CHATTERBOX_EMAIL_RECEIVER");
        let sender_name = std::env::var("CHATTERBOX_EMAIL_NAME").unwrap_or("noreply".to_string());
        let sender_address = std::env::var("CHATTERBOX_EMAIL_SENDER").unwrap_or(smtp_user.clone());

        let test_message = Message::test_example();
        send_message(
            &smtp_server,
            &smtp_user,
            &smtp_password,
            &sender_address,
            &sender_name,
            &receiver_address,
            test_message,
        )
        .await
        .unwrap();
    }
}
