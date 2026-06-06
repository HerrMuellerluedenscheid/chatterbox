Chatterbox
==========

Streamlined notifications via:

 * Email (SMTP)
 * Email (Resend HTTP API)
 * Telegram
 * Slack
 * Discord (bot token, or incoming webhook)
 * Gotify
 * Microsoft Teams
 * ntfy
 * Pushover
 * Matrix
 * Generic webhook (POST the message as JSON to any URL)

A simple message consists only of a title and a body, which provides a common
interface for all notification channels.

How it works
------------

Chatterbox is built around three pieces:

 * **`Sender`** — declarative config: an at-most-one `Option<_>` per transport.
   You fill in only the channels you want; the rest stay `None`.
 * **`Dispatcher`** — the runtime. Constructing one from a `Sender` spins up a
   background tokio task per configured transport, all subscribed to a shared
   broadcast channel.
 * **`Message`** (and the `Notification` trait) — the payload. A `Message` is
   just a `title` + `body`; each transport renders it in its own way (HTML for
   Telegram, an embed for Discord, a subject/body for email, …).

When you `dispatch(...)`, the message is serialized once and pushed onto the
broadcast channel; every transport's task picks it up and delivers it
concurrently. Delivery is fire-and-forget — a failing channel logs an error
but never blocks the others or the caller.

Usage
-----

```rust
use chatterbox::dispatcher::Sender;
use chatterbox::dispatcher::telegram::Telegram;
use chatterbox::dispatcher::discord_webhook::DiscordWebhook;
use chatterbox::message::{Dispatcher, Message};

#[tokio::main]
async fn main() {
    // 1. Describe the channels you want to deliver to.
    let sender = Sender {
        telegram: Some(Telegram {
            bot_token: std::env::var("TELEGRAM_BOT_TOKEN").unwrap(),
            chat_id: 1234567890,
        }),
        discord_webhook: Some(DiscordWebhook {
            webhook_url: std::env::var("DISCORD_WEBHOOK_URL").unwrap(),
            username: Some("Chatterbox".to_string()),
            avatar_url: None,
        }),
        ..Default::default()
    };

    // 2. Build the dispatcher. This validates each channel's config and
    //    starts a background delivery task for each one. (Construction
    //    panics if a config fails validation — call `sender.check()`
    //    yourself first if you want to handle that gracefully.)
    let dispatcher = Dispatcher::new(sender);

    // 3. Dispatch messages from anywhere. Cheap and non-blocking.
    let msg = Message::new("Deploy finished".into(), "v1.4.2 is live".into());
    dispatcher.dispatch(&msg).expect("dispatch failed");

    // Or send a canned connectivity check to every configured channel:
    dispatcher.send_test_message().ok();
}
```

`dispatch` accepts anything implementing the `Notification` trait, so you can
implement it on your own domain types and hand them straight to the dispatcher
instead of building a `Message` by hand:

```rust
use chatterbox::message::{Message, Notification};

struct DeployEvent { service: String, version: String, ok: bool }

impl Notification for DeployEvent {
    fn message(&self) -> Message {
        let status = if self.ok { "succeeded" } else { "ROLLED BACK" };
        Message::new(
            format!("{} deploy {}", self.service, status),
            format!("version {}", self.version),
        )
    }
}

// dispatcher.dispatch(&DeployEvent { .. })?;
```

### One transport per kind

A `Sender` holds at most one of each transport. If you need to fan a message
out to, say, three different Discord channels, build one single-transport
`Sender` (and therefore one `Dispatcher`) per target and dispatch to each —
this is exactly what [hoister](https://github.com/HerrMuellerluedenscheid/hoister)
does to back a "user has many notifiers" data model on top of chatterbox: it
translates each stored notifier into a one-kind `Sender`, constructs a
throwaway `Dispatcher`, and dispatches the event, logging and swallowing any
per-channel error so one broken webhook can't hold up the rest.

### Choosing a Discord transport

Two Discord transports are available:

 * **`discord::Discord`** — posts with a **bot token** to a specific
   `channel_id`. Use this if you already run a bot and want to target channels
   dynamically.
 * **`discord_webhook::DiscordWebhook`** — posts to an **incoming webhook URL**
   (`https://discord.com/api/webhooks/{id}/{token}`). No bot, no gateway; the
   message is delivered as the app/integration that owns the webhook, and the
   target channel is fixed when the webhook is created. `username` and
   `avatar_url` optionally override the webhook's default identity per message.
   This is the simplest way to push notifications into a channel.

### Self-hosted and generic transports

 * **`ntfy::Ntfy`** — publishes to an [ntfy](https://ntfy.sh) topic on the
   public service or your own server (`server_url` + `topic`). `access_token`
   is only needed for protected/reserved topics. The sibling of Gotify for
   simple pub/sub push.
 * **`pushover::Pushover`** — fans a message out to all of a user's devices via
   [Pushover](https://pushover.net). Needs an application `token` and the
   recipient `user` (or group) key; `device` optionally narrows it to one
   device.
 * **`matrix::Matrix`** — sends an `m.room.message` to a single room on any
   Matrix homeserver (`homeserver_url` + `room_id`) using a long-lived
   `access_token`. Open, federated and self-hostable; the message is sent with
   HTML formatting so titles render in bold.
 * **`webhook::Webhook`** — the escape hatch: POSTs the `Message` as JSON
   (`{"title","body","subject"}`) to any `url`, with optional `headers` for
   auth. Use it to reach automation platforms (Zapier, n8n, Make, IFTTT) or any
   custom endpoint without a dedicated transport.
