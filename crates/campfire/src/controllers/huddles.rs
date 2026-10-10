//! Huddles (plans/huddles.md): joining a room's call, and LiveKit's webhooks about who's in it.

use campfire_db::{Message, NewMessage, Room};
use ruby_compat::integer_cast;
use campfire_kit::{Ctx, Error, Result, StatusCode, format};
use serde_json::json;

use crate::app::{App, AppCtx};
use crate::concerns::{self, Before, before_actions, require_current_user};
use crate::controllers::messages::{broadcast_create, by_bots::raw_request_body};
use crate::controllers::presenters::{avatar_path, page, page::db_error};
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
    let thread_url = huddles::announcement(room.id).map(|message_id| campfire_routes::room_thread(room.id, message_id));
    let body = json!({ "url": settings.url, "token": token, "participants": participants, "thread_url": thread_url }).to_string();
    Ok(c.render(StatusCode::OK, &format::JSON, body))
}

/// `GET /rooms/:room_id/huddle/people`: the room's other people, for "Add people" in a huddle.
pub async fn people(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let me = require_current_user(c)?.id;
    let in_huddle: Vec<i64> = huddles::participants(room.id).iter().map(|person| person.user_id).collect();
    let app = c.app().clone();
    let room_id = room.id;
    let people = c
        .app()
        .db
        .read(move |conn| {
            let mut people = Vec::new();
            for membership in campfire_db::Membership::for_room(conn, room_id)? {
                let Ok(user) = campfire_db::User::find(conn, membership.user_id) else { continue };
                if user.id == me || user.role == campfire_db::Role::Bot || user.status != campfire_db::Status::Active {
                    continue;
                }
                people.push(json!({ "id": user.id, "name": user.name, "avatar_url": avatar_path(&app.secrets, &user), "in_huddle": in_huddle.contains(&user.id) }));
            }
            people.sort_by_key(|person| person["name"].as_str().unwrap_or_default().to_lowercase());
            Ok(people)
        })
        .await
        .map_err(db_error)?;
    Ok(c.render(StatusCode::OK, &format::JSON, serde_json::Value::from(people).to_string()))
}

/// `POST /rooms/:room_id/huddle/ring`: asks one of the room's people (`user_id`) to join its
/// huddle, as Slack's "Add people" does: they get a ring and a Join button, wherever they are.
pub async fn ring(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let caller = require_current_user(c)?.clone();
    let Some(user_id) = c.param_str("user_id").and_then(integer_cast) else { return Err(Error::NotFound) };
    let (app, room_id) = (c.app().clone(), room.id);
    let avatar_url = avatar_path(&c.app().secrets, &caller);
    let rung = c
        .app()
        .db
        .read(move |conn| {
            let Some(_) = campfire_db::Membership::find_by_room_and_user(conn, room_id, user_id)? else { return Ok(false) };
            let them = campfire_db::User::find(conn, user_id)?;
            let room_name = crate::controllers::presenters::accounts::room_display_name(conn, &room, &them)?;
            app.broadcasts.huddle_ring(user_id, &json!({ "ring": { "room_id": room_id, "room_name": room_name, "direct": room.direct(), "from": { "name": caller.name, "avatar_url": avatar_url } } }));
            Ok(true)
        })
        .await
        .map_err(db_error)?;
    if !rung {
        return Err(Error::NotFound);
    }
    Ok(concerns::head(StatusCode::NO_CONTENT))
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
    huddles::announced(room_id, message.id, huddles::unix_now());
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

/// Tells the room's members who's in its huddle now; when it's over, its card says so.
pub(crate) async fn broadcast_huddle(app: &App, room_id: i64) -> Result<()> {
    let participants = huddles::participants(room_id);
    let ended = huddles::track(room_id, &participants, huddles::unix_now());
    let reader = app.clone();
    app.db.read(move |conn| reader.broadcasts.huddle(conn, room_id, &participants)).await.map_err(db_error)?;
    if let Some(session) = ended
        && let Some(message_id) = session.message_id
    {
        end_card(app, room_id, message_id, huddles::ended_body(&session, huddles::unix_now())).await?;
    }
    Ok(())
}

/// Turns the room's "started a huddle" message into "Huddle ended · lasted …", for everyone.
async fn end_card(app: &App, room_id: i64, message_id: i64, body: String) -> Result<()> {
    let message = app
        .db
        .write(move |tx| {
            let mut message = Message::find(tx.conn(), message_id)?;
            message.update_body(tx, &body)?;
            Ok(message)
        })
        .await
        .map_err(db_error)?;
    let app = app.clone();
    app.clone()
        .db
        .read(move |conn| {
            let room = Room::find(conn, room_id)?;
            crate::controllers::messages::broadcast_replace_on(conn, &app, &room, &message, page::DETACHED_BASE_URL)
        })
        .await
        .map_err(db_error)
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
