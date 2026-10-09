//! Scheduled messages (plans/scheduled-messages.md): write now, post later. A JSON API for the
//! composer's "Schedule" menu and the panel listing yours; the posting is [`deliver_in_background`].
use campfire_db::{Message, NewScheduledMessage, Room, ScheduledMessage};
use campfire_kit::{Ctx, Error, Result, StatusCode, format};
use serde_json::{Value, json};

use crate::app::{App, AppCtx};
use crate::concerns::{self, Before, before_actions, cast_integer, require_current_user};
use crate::controllers::messages::{broadcast_create_on, canonicalize_body, deliver_webhooks_to_bots_on, plain_text_of, thread_root_id};
use crate::controllers::presenters::page::{self, db_error};

/// Not sooner than this (the composer's "send" is for now) nor later than about four months.
const SOONEST_SECONDS: i64 = 30;
const LATEST_SECONDS: i64 = 120 * 24 * 60 * 60;
const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

fn now() -> i64 {
    crate::huddles::unix_now()
}

fn json_of(app: &App, conn: &campfire_db::Connection, scheduled: &ScheduledMessage) -> Value {
    json!({
        "id": scheduled.id,
        "room_id": scheduled.room_id,
        "thread_parent_id": scheduled.thread_parent_id,
        "send_at": scheduled.send_at,
        "preview": plain_text_of(conn, app, &scheduled.body).chars().take(280).collect::<String>(),
    })
}

fn render_json(c: &mut Ctx, status: StatusCode, value: Value) -> Result {
    Ok(c.render(status, &format::JSON, value.to_string()))
}

/// `send_at`, or why it won't do.
fn send_at_param(c: &Ctx) -> std::result::Result<i64, &'static str> {
    let send_at = c.param_str("send_at").and_then(|value| value.trim().parse::<i64>().ok()).ok_or("Pick a time to send it")?;
    let now = now();
    if send_at < now + SOONEST_SECONDS {
        return Err("Pick a time in the future");
    }
    if send_at > now + LATEST_SECONDS {
        return Err("Messages can be scheduled up to 120 days ahead");
    }
    Ok(send_at)
}

fn unprocessable(c: &mut Ctx, reason: &str) -> Result {
    render_json(c, StatusCode::UNPROCESSABLE_ENTITY, json!({ "error": reason }))
}

async fn find_mine(c: &mut Ctx, room: &Room) -> Result<ScheduledMessage> {
    let id = c.param_str("id").and_then(cast_integer).ok_or(Error::NotFound)?;
    let (user_id, room_id) = (require_current_user(c)?.id, room.id);
    let scheduled = c.app().db.read(move |conn| ScheduledMessage::find(conn, id)).await.map_err(|_| Error::NotFound)?;
    if scheduled.creator_id != user_id || scheduled.room_id != room_id || scheduled.message_id.is_some() {
        return Err(Error::NotFound);
    }
    Ok(scheduled)
}

/// `GET /rooms/:room_id/scheduled_messages(.json)`: yours, waiting, in this room (`?all=1`: in any).
pub async fn index(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let user_id = require_current_user(c)?.id;
    let room_id = if c.param_str("all").is_some() { None } else { Some(room.id) };
    let app = c.app().clone();
    let list = c
        .app()
        .db
        .read(move |conn| {
            let pending = ScheduledMessage::pending_for(conn, user_id, room_id)?;
            Ok(pending.iter().map(|scheduled| json_of(&app, conn, scheduled)).collect::<Vec<_>>())
        })
        .await
        .map_err(db_error)?;
    render_json(c, StatusCode::OK, json!(list))
}

/// `POST /rooms/:room_id/scheduled_messages`: `body` (the composer's HTML), `send_at` (Unix
/// seconds), and for a reply, `thread_parent_id`.
pub async fn create(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let creator_id = require_current_user(c)?.id;
    let send_at = match send_at_param(c) {
        Ok(send_at) => send_at,
        Err(reason) => return unprocessable(c, reason),
    };
    let body = c.param_str("body").unwrap_or_default().to_string();
    if body.trim().is_empty() {
        return unprocessable(c, "Write something to schedule");
    }
    let body = canonicalize_body(c.app(), body, Some(c.request.host())).await?;
    let requested_parent = c.param_str("thread_parent_id").and_then(cast_integer);
    let room_id = room.id;
    let app = c.app().clone();
    let created = c
        .app()
        .db
        .write(move |tx| {
            let thread_parent_id = requested_parent.map(|id| thread_root_id(tx.conn(), room_id, id)).transpose()?;
            ScheduledMessage::create(tx, NewScheduledMessage { room_id, creator_id, thread_parent_id, body, send_at }, now())
        })
        .await
        .map_err(db_error)?;
    let value = c.app().db.read(move |conn| Ok(json_of(&app, conn, &created))).await.map_err(db_error)?;
    render_json(c, StatusCode::CREATED, value)
}

/// `PATCH /rooms/:room_id/scheduled_messages/:id`: a new `send_at`.
pub async fn update(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let mut scheduled = find_mine(c, &room).await?;
    let send_at = match send_at_param(c) {
        Ok(send_at) => send_at,
        Err(reason) => return unprocessable(c, reason),
    };
    let app = c.app().clone();
    let value = c
        .app()
        .db
        .write(move |tx| {
            scheduled.reschedule(tx, send_at, None)?;
            Ok(json_of(&app, tx.conn(), &scheduled))
        })
        .await
        .map_err(db_error)?;
    render_json(c, StatusCode::OK, value)
}

/// `DELETE /rooms/:room_id/scheduled_messages/:id`
pub async fn destroy(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let scheduled = find_mine(c, &room).await?;
    c.app().db.write(move |tx| scheduled.destroy(tx)).await.map_err(db_error)?;
    Ok(concerns::head(StatusCode::NO_CONTENT))
}

/// `POST /rooms/:room_id/scheduled_messages/:id/deliver`: send it now.
pub async fn deliver(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let scheduled = find_mine(c, &room).await?;
    post(c.app(), scheduled.id).await?;
    Ok(concerns::head(StatusCode::NO_CONTENT))
}

/// Posts one, if it's still waiting, and tells everyone as any new message does.
async fn post(app: &App, id: i64) -> Result<Option<Message>> {
    let Some(message) = app.db.write(move |tx| ScheduledMessage::post(tx, id)).await.map_err(db_error)? else {
        return Ok(None);
    };
    let (reader, room_id, id) = (app.clone(), message.room_id, message.id);
    let (room, message) = app
        .db
        .read(move |conn| {
            let room = Room::find(conn, room_id)?;
            let message = Message::find(conn, id)?;
            broadcast_create_on(conn, &reader, &room, &message, page::DETACHED_BASE_URL)?;
            Ok((room, message))
        })
        .await
        .map_err(db_error)?;
    deliver_webhooks_to_bots_on(app, &room, &message).await?;
    Ok(Some(message))
}

/// Posts what's due, every few seconds, from boot (so anything that came due while the app was
/// down goes out as soon as it's back).
pub fn deliver_in_background(app: App) {
    tokio::spawn(async move {
        loop {
            match app.db.read(|conn| ScheduledMessage::due(conn, now(), 50)).await {
                Ok(due) => {
                    for id in due {
                        if let Err(error) = post(&app, id).await {
                            tracing::warn!(?error, id, "couldn't post a scheduled message");
                        }
                    }
                }
                Err(error) => tracing::warn!(%error, "couldn't look for scheduled messages"),
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}
