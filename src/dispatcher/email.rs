use crate::dispatcher::{DispatchError, Example, Handler};
use crate::message::Message;
use lettre::transport::smtp::authentication::Credentials;
use lettre::Message as LettreMessage;
use lettre::{SmtpTransport, Transport};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Receiver;
use validator::Validate;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Validate)]
pub struct Email {
    pub smtp_user: String,
    pub smtp_password: String,
    pub smtp_server: String,

    #[validate(email)]
    pub receiver_address: String,
}

impl Example for Email {
    fn example() -> Self {
        Self {
            smtp_user: "USERNAME".to_string(),
            smtp_password: "SUPERSECUREPASSWORD".to_string(),
            smtp_server: "".to_string(),
            receiver_address: "foo.bar@x.x".to_string(),
        }
    }
}

impl Handler for Email {
    fn check(&self) -> Result<(), DispatchError> {
        self.validate().map_err(DispatchError::ValidationError)
    }

    fn start_handler(self, receiver: Receiver<String>) {
        let mut email_handler = EmailHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            email_handler.start().await;
        });
        debug!("started email handlers");
    }
}

pub struct EmailHandler {
    pub(crate) config: Email,
    pub(crate) receiver: Receiver<String>,
}

impl EmailHandler {
    /// Dispatch an email
    async fn send(&self, message: Message) {
        let config = &self.config;
        let email = LettreMessage::builder()
            .from("Chatterbox <noreply@intrusion.detection>".parse().unwrap())
            .reply_to("noreply@intrusion.detection".parse().unwrap())
            .to(config.receiver_address.parse().unwrap())
            .subject(message.body.clone())
            .body(message.html())
            .unwrap();

        let credentials = Credentials::new(config.smtp_user.clone(), config.smtp_password.clone());

        let mailer = SmtpTransport::relay(&config.smtp_server)
            .unwrap()
            .credentials(credentials)
            .build();

        match mailer.send(&email) {
            Ok(_) => debug!("Email sent successfully"),
            Err(e) => error!("Could not send email: {:?}", e),
        }
    }

    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            self.send(message).await;
        }
    }
}

#[test]
fn test_example() {
    Email::example();
}
