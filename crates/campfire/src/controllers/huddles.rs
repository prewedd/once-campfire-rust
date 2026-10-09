//! Huddles (plans/huddles.md): joining a room's call, and LiveKit's webhooks about who's in it.

use campfire_db::{Message, NewMessage, Room};
use campfire_kit::{Ctx, Error, Result, StatusCode, format};
use serde_json::json;

use crate::app::{App, AppCtx};
use crate::concerns::{self, Before, before_actions, require_current_user};
use crate::controllers::messages::{broadcast_create, by_bots::raw_request_body};
use crate::controllers::presenters::{avatar_path, page::db_error};
use crate::huddles::{self, SETTINGS};

/// `POST /rooms/:room_id/huddle`: a token to join the room's huddle, for its members. With
/// `announce`, starting an empty huddle also posts "🎧 started a huddle", so the room hears of it
/// (and is pushed it) the way it hears of any message.
pub async fn join(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let Some(settings) = SETTINGS.as_ref() else { return Err(Error::NotFound) };
    let user = require_current_user(c)?.clone();
    let avatar_url = avatar_path(&c.app().secrets, &user);
    let token = huddles::join_token(settings, user.id, &user.name, &avatar_url, room.id, huddles::unix_now());
    let participants = huddles::participants(room.id);

    let announce = c.param_str("announce").is_some_and(|value| value == "1" || value == "true");
    if announce && participants.is_empty() {
        announce_start(c, &room, user.id).await?;
    }

    c.respond_to(&[&format::JSON])?;
    let body = json!({ "url": settings.url, "token": token, "participants": participants }).to_string();
    Ok(c.render(StatusCode::OK, &format::JSON, body))
}

async fn announce_start(c: &Ctx, room: &Room, creator_id: i64) -> Result<()> {
    let room_id = room.id;
    let message = c
        .app()
        .db
        .write(move |tx| {
            Message::create(
                tx,
                NewMessage {
                    room_id,
                    creator_id,
                    client_message_id: None,
                    body: Some("<p>🎧 started a huddle</p>".into()),
                    attachment_blob_id: None,
                    thread_parent_id: None,
                },
            )
        })
        .await
        .map_err(db_error)?;
    broadcast_create(c, room, &message).await
}

/// `POST /livekit/webhook`: LiveKit telling us someone joined or left a huddle. Signed by
/// LiveKit with the API secret (over the body's SHA-256); anything else is a 401.
pub async fn webhook(c: &mut Ctx) -> Result {
    before_actions(c, Before::default().allow_unauthenticated_access().skip_forgery_protection()).await?;
    let Some(settings) = SETTINGS.as_ref() else { return Err(Error::NotFound) };
    let authorization = c.request.header("authorization").unwrap_or_default().to_string();
    let body = raw_request_body(c);
    let Some(event) = huddles::verify_webhook(settings, &authorization, body.as_bytes(), huddles::unix_now()) else {
        return Ok(concerns::head(StatusCode::UNAUTHORIZED));
    };
    if let Some(room_id) = huddles::apply(&event) {
        broadcast_huddle(c.app(), room_id).await?;
    }
    Ok(concerns::head(StatusCode::OK))
}

/// Tells the room's members who's in its huddle now.
pub(crate) async fn broadcast_huddle(app: &App, room_id: i64) -> Result<()> {
    let participants = huddles::participants(room_id);
    let app = app.clone();
    app.clone().db.read(move |conn| app.broadcasts.huddle(conn, room_id, &participants)).await.map_err(db_error)
}

/// Keeps in step with LiveKit: on boot, then every minute (webhooks are best effort).
pub fn reconcile_in_background(app: App) {
    let Some(settings) = SETTINGS.as_ref() else { return };
    tokio::spawn(async move {
        loop {
            match huddles::fetch(settings).await {
                Ok(current) => {
                    for room_id in huddles::replace(current) {
                        if let Err(error) = broadcast_huddle(&app, room_id).await {
                            tracing::warn!(?error, room_id, "huddle broadcast failed");
                        }
                    }
                }
                Err(error) => tracing::warn!(%error, "couldn't list LiveKit's huddles"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}
