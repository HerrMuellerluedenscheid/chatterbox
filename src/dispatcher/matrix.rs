use crate::dispatcher::{DispatchError, Example, Handler};
use crate::message::Message;
use log::error;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast::Receiver;
use validator::Validate;

/// Delivery to a [Matrix](https://matrix.org) room via the client-server API.
///
/// Matrix is an open, federated, self-hostable chat protocol. This dispatcher
/// sends an `m.room.message` event to a single room using a long-lived
/// `access_token` (obtained by logging the sending user/bot in once). It posts
/// an HTML-formatted message so titles render in bold in clients that support
/// formatting, falling back to plain text elsewhere.
///
/// See the [send-event endpoint][send].
///
/// [send]: https://spec.matrix.org/latest/client-server-api/#put_matrixclientv3roomsroomidsendeventtypetxnid
#[derive(Validate, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Matrix {
    /// Base URL of the homeserver, e.g. `https://matrix.org`.
    pub homeserver_url: Url,

    /// Access token of the sending user/bot (`Authorization: Bearer <token>`).
    pub access_token: String,

    /// Target room id (`!abcdef:example.org`) or alias the token has joined.
    pub room_id: String,
}

impl Example for Matrix {
    fn example() -> Self {
        Self {
            homeserver_url: Url::from_str("https://matrix.org").unwrap(),
            access_token: "syt_xxxxxxxxxxxxxxxxxxxx".to_string(),
            room_id: "!roomid:matrix.org".to_string(),
        }
    }
}

impl Handler for Matrix {
    fn check(&self) -> Result<(), DispatchError> {
        self.validate().map_err(DispatchError::ValidationError)
    }

    fn start_handler(self, receiver: Receiver<String>) {
        let mut handler = MatrixHandler {
            config: self,
            receiver,
        };
        tokio::spawn(async move {
            handler.start().await;
        });
    }
}

#[derive(Serialize)]
struct MatrixPayload {
    msgtype: &'static str,
    body: String,
    format: &'static str,
    formatted_body: String,
}

/// Build the `PUT .../send/m.room.message/{txn}` URL, percent-encoding the room
/// id and transaction id as path segments.
fn send_url(homeserver: &Url, room_id: &str, txn_id: &str) -> Result<Url, String> {
    let mut url = homeserver.clone();
    url.path_segments_mut()
        .map_err(|_| "homeserver_url cannot be a base".to_string())?
        .pop_if_empty()
        .extend([
            "_matrix",
            "client",
            "v3",
            "rooms",
            room_id,
            "send",
            "m.room.message",
            txn_id,
        ]);
    Ok(url)
}

/// A transaction id unique per request so the homeserver can deduplicate
/// retries. Fire-and-forget delivery never retries, so a monotonic timestamp is
/// sufficient.
fn transaction_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("chatterbox-{nanos}")
}

/// Send an `m.room.message` event to a Matrix room.
///
/// Returns the upstream status and body on a non-2xx response so callers can
/// surface a useful error (invalid/expired token, not joined to the room, …).
pub async fn send_message(
    homeserver_url: &Url,
    access_token: &str,
    room_id: &str,
    message: Message,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let url = send_url(homeserver_url, room_id, &transaction_id())?;

    let payload = MatrixPayload {
        msgtype: "m.text",
        body: format!("{}\n\n{}", message.title, message.body),
        format: "org.matrix.custom.html",
        formatted_body: message.html(),
    };

    let response = client
        .put(url)
        .bearer_auth(access_token)
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Matrix API error: {} - {}", status, error_text).into());
    }

    Ok(())
}

pub struct MatrixHandler {
    pub(crate) config: Matrix,
    pub(crate) receiver: Receiver<String>,
}

impl MatrixHandler {
    pub async fn start(&mut self) {
        while let Ok(data) = self.receiver.recv().await {
            let message = Message::from_json(data);
            if let Err(e) = send_message(
                &self.config.homeserver_url,
                &self.config.access_token,
                &self.config.room_id,
                message,
            )
            .await
            {
                error!("failed sending via Matrix: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        Matrix::example();
    }

    #[test]
    fn send_url_encodes_room_id() {
        let homeserver = Url::from_str("https://matrix.org/").unwrap();
        let url = send_url(&homeserver, "!abc:matrix.org", "txn-1").unwrap();
        assert_eq!(
            url.as_str(),
            "https://matrix.org/_matrix/client/v3/rooms/!abc:matrix.org/send/m.room.message/txn-1"
        );

        // A trailing slash on the homeserver must not produce a double slash.
        let no_slash = Url::from_str("https://matrix.org").unwrap();
        assert_eq!(
            send_url(&no_slash, "!abc:matrix.org", "txn-1").unwrap(),
            url
        );

        // Alias rooms start with '#', which must be percent-encoded in a path.
        let alias = send_url(&homeserver, "#room:matrix.org", "txn-1").unwrap();
        assert!(alias.as_str().contains("/rooms/%23room:matrix.org/send/"));
    }

    #[tokio::test]
    #[ignore = "Requires env vars CHATTERBOX_MATRIX_HOMESERVER_URL, CHATTERBOX_MATRIX_ACCESS_TOKEN and CHATTERBOX_MATRIX_ROOM_ID"]
    async fn test_dispatch_example() {
        let homeserver_url = std::env::var("CHATTERBOX_MATRIX_HOMESERVER_URL")
            .expect("missing env var CHATTERBOX_MATRIX_HOMESERVER_URL");
        let access_token = std::env::var("CHATTERBOX_MATRIX_ACCESS_TOKEN")
            .expect("missing env var CHATTERBOX_MATRIX_ACCESS_TOKEN");
        let room_id = std::env::var("CHATTERBOX_MATRIX_ROOM_ID")
            .expect("missing env var CHATTERBOX_MATRIX_ROOM_ID");

        let homeserver_url = Url::from_str(&homeserver_url).unwrap();
        let test_message = Message::test_example();
        send_message(&homeserver_url, &access_token, &room_id, test_message)
            .await
            .unwrap();
    }
}
