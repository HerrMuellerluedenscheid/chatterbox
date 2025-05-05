use crate::dispatcher::Sender;
use log::debug;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::SendError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub title: String,
    pub body: String,
}

impl Message {
    pub fn new(title: String, body: String) -> Self {
        Message { title, body }
    }

    pub(crate) fn as_json(&self) -> String {
        serde_json::to_string(&self).unwrap()
    }

    pub(crate) fn from_json(data: String) -> Self {
        serde_json::from_str(&data).expect("failed json message")
    }

    pub(crate) fn html(&self) -> String {
        format!("<b>{}</b>{}", self.title, self.body)
    }

    pub(crate) fn markdown(&self) -> String {
        format!("#{}\n{}\n", self.title, self.body)
    }

    pub(crate) fn test_example() -> Self {
        Self {
            title: "Test Message".to_string(),
            body: "This message was sent to test connectivity".to_string(),
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
