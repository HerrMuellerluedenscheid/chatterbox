use crate::dispatcher::{DispatchError, Example, Handler};
use crate::message::Message;
use log::error;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Receiver;
use validator::Validate;

/// Email delivery through the [Resend](https://resend.com) HTTP API.
///
/// Unlike [`Email`](crate::dispatcher::email::Email), which opens an SMTP
/// connection per message, this dispatcher POSTs to `api.resend.com` with a
/// bearer API key. `from` must be an address on a domain verified in the
/// Resend dashboard.
#[derive(Validate, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Resend {
    pub api_key: String,

    /// Sender identity, on a Resend-verified domain. Resend accepts both a
    /// bare `addr@domain` and the `Name <addr@domain>` form, so this is not
    /// constrained to a plain address by the validator.
    pub from: String,

    #[validate(email)]
    pub to: String,
}

impl Example for Resend {
    fn example() -> Self {
        Self {
            api_key: "re_xxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
            from: "noreply@example.com".to_string(),
            to: "foo.bar@example.com".to_string(),
        }
    }
}

impl Handler for Resend {
    fn check(&self) -> Result<(), DispatchError> {
        self.validate().map_err(DispatchError::ValidationError)
    }

    fn start_handler(self, receiver: Receiver<String>) {
        let mut handler = ResendHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            handler.start().await;
        });
    }
}

#[derive(Serialize)]
struct ResendPayload<'a> {
    from: &'a str,
    to: [&'a str; 1],
    subject: &'a str,
    html: String,
    text: String,
    headers: ResendHeaders,
}

/// Custom SMTP headers Resend forwards verbatim. Anchoring `References` /
/// `In-Reply-To` to a subject-derived id makes the recipient's client thread
/// notifications that share a subject into one conversation, mirroring the SMTP
/// [`Email`](crate::dispatcher::email::Email) dispatcher.
#[derive(Serialize)]
struct ResendHeaders {
    #[serde(rename = "References")]
    references: String,
    #[serde(rename = "In-Reply-To")]
    in_reply_to: String,
}

/// Send an email through the Resend API.
///
/// See <https://resend.com/docs/api-reference/emails/send-email>. Returns the
/// upstream status and body on a non-2xx response so callers can surface a
/// useful error (invalid key, unverified `from` domain, …).
pub async fn send_message(
    api_key: &str,
    from: &str,
    to: &str,
    message: Message,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let thread_id = message.thread_message_id();
    let payload = ResendPayload {
        from,
        to: [to],
        subject: message.subject_line(),
        html: message.html(),
        text: message.markdown(),
        headers: ResendHeaders {
            references: thread_id.clone(),
            in_reply_to: thread_id,
        },
    };

    let response = client
        .post("https://api.resend.com/emails")
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Resend API error: {} - {}", status, error_text).into());
    }

    Ok(())
}

pub struct ResendHandler {
    pub(crate) config: Resend,
    pub(crate) receiver: Receiver<String>,
}

impl ResendHandler {
    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            if let Err(e) = send_message(
                &self.config.api_key,
                &self.config.from,
                &self.config.to,
                message,
            )
            .await
            {
                error!("failed sending via Resend: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        Resend::example();
    }

    #[tokio::test]
    #[ignore = "Requires env vars CHATTERBOX_RESEND_API_KEY, CHATTERBOX_RESEND_FROM and CHATTERBOX_RESEND_TO"]
    async fn test_dispatch_example() {
        let api_key = std::env::var("CHATTERBOX_RESEND_API_KEY")
            .expect("missing env var CHATTERBOX_RESEND_API_KEY");
        let from = std::env::var("CHATTERBOX_RESEND_FROM")
            .expect("missing env var CHATTERBOX_RESEND_FROM");
        let to =
            std::env::var("CHATTERBOX_RESEND_TO").expect("missing env var CHATTERBOX_RESEND_TO");

        let test_message = Message::test_example();
        send_message(&api_key, &from, &to, test_message)
            .await
            .unwrap();
    }
}
