use crate::dispatcher::Sender;
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::SendError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub title: String,
    pub body: String,
    /// Optional subject / thread key for email-style dispatchers (SMTP,
    /// Resend). When set it overrides `title` as the email `Subject` and seeds
    /// the `References`/`In-Reply-To` headers, so notifications that share a
    /// subject group into one conversation in the recipient's mail client.
    /// Chat dispatchers ignore it and keep using `title` as their heading.
    /// `None` falls back to `title`.
    #[serde(default)]
    pub subject: Option<String>,
}

impl Message {
    pub fn new(title: String, body: String) -> Self {
        Message {
            title,
            body,
            subject: None,
        }
    }

    /// Set an explicit email subject / thread key, returning `self` for
    /// chaining. See [`Message::subject`] for how it is used.
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// Subject line for email-style dispatchers: the explicit [`subject`](Self::subject)
    /// when present, otherwise the `title`.
    pub(crate) fn subject_line(&self) -> &str {
        self.subject.as_deref().unwrap_or(&self.title)
    }

    /// Deterministic RFC 5322 message-id derived from the subject line. Email
    /// dispatchers attach it as a shared `References`/`In-Reply-To` anchor so
    /// every message with the same subject threads into one conversation, even
    /// in clients (e.g. Thunderbird) that don't group by subject alone. The
    /// `.invalid` TLD (RFC 2606) keeps the synthetic id from ever colliding
    /// with a real domain.
    pub(crate) fn thread_message_id(&self) -> String {
        let mut hasher = DefaultHasher::new();
        self.subject_line().hash(&mut hasher);
        format!(
            "<chatterbox-thread-{:016x}@chatterbox.invalid>",
            hasher.finish()
        )
    }

    pub(crate) fn as_json(&self) -> String {
        serde_json::to_string(&self).unwrap()
    }

    pub(crate) fn from_json(data: String) -> Self {
        serde_json::from_str(&data).expect("failed json message")
    }

    pub(crate) fn html(&self) -> String {
        format!("<b>{}</b>\n\n{}", self.title, self.body)
    }

    pub(crate) fn markdown(&self) -> String {
        format!("#{}\n{}\n", self.title, self.body)
    }

    pub(crate) fn test_example() -> Self {
        Self {
            title: "Test Message".to_string(),
            body: "This message was sent to test connectivity".to_string(),
            subject: None,
        }
    }
}

impl Notification for Message {
    fn message(&self) -> Message {
        self.clone()
    }
}

pub struct Dispatcher {
    tx: broadcast::Sender<String>,
}

impl Dispatcher {
    pub fn new(sender: Sender) -> Self {
        let (tx, _) = broadcast::channel::<String>(100);

        sender
            .setup_dispatcher(&tx)
            .expect("setting up dispatcher failed");
        debug!("created sender channel");
        Self { tx }
    }

    pub fn dispatch<T: Notification>(&self, notification: &T) -> Result<(), SendError<String>> {
        if self.tx.receiver_count() == 0 {
            debug!("no receivers connected");
            return Ok(());
        }
        debug!("dispatching message");
        let message = notification.message();
        self.tx.send(message.as_json())?;
        Ok(())
    }

    pub fn send_test_message(&self) -> Result<(), SendError<String>> {
        let message = Message::test_example();
        self.dispatch(&message)
    }

    pub fn stop(self) {
        drop(self.tx);
    }
}

/// Structs implementing this trait can be dispatched with the [Dispatcher](Dispatcher).
pub trait Notification {
    /// An implementation of this method returns a `String` that will be dispatched to the user.
    fn message(&self) -> Message;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_line_falls_back_to_title() {
        let m = Message::new("the title".into(), "body".into());
        assert_eq!(m.subject_line(), "the title");
        assert_eq!(m.with_subject("explicit").subject_line(), "explicit");
    }

    #[test]
    fn thread_id_tracks_subject_not_title_or_body() {
        // The threading anchor must depend only on the subject so messages that
        // share a subject group together regardless of their title/body, while
        // a different subject starts a new conversation.
        let a = Message::new("title".into(), "body".into()).with_subject("image-x");
        let a_other =
            Message::new("other title".into(), "other body".into()).with_subject("image-x");
        let b = Message::new("title".into(), "body".into()).with_subject("image-y");

        assert_eq!(a.thread_message_id(), a_other.thread_message_id());
        assert_ne!(a.thread_message_id(), b.thread_message_id());

        let id = a.thread_message_id();
        assert!(
            id.starts_with('<') && id.ends_with('>'),
            "must be an RFC 5322 msg-id: {id}"
        );
    }

    #[test]
    fn message_survives_json_roundtrip_with_and_without_subject() {
        // `#[serde(default)]` keeps payloads from older senders (no `subject`)
        // wire-compatible across the broadcast channel.
        let with = Message::new("t".into(), "b".into()).with_subject("s");
        assert_eq!(
            Message::from_json(with.as_json()).subject.as_deref(),
            Some("s")
        );

        let legacy = r#"{"title":"t","body":"b"}"#.to_string();
        assert_eq!(Message::from_json(legacy).subject, None);
    }
}
