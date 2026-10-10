//! Pinned messages (plans/redesign.md), this app's own: pin from a message's "⋯" menu
//! (lib/workspace/pins.js); a channel's pins open from the "📌" in its header.
use campfire_db::PinnedMessage;
use campfire_kit::{Ctx, Error, Result, StatusCode, format};
use campfire_views::searches::Pinned;

use crate::app::AppCtx;
use crate::concerns::{self, Before, before_actions, cast_integer, require_current_user};
use crate::controllers::messages::present;
use crate::controllers::presenters::page::{self, db_error};

/// `GET /rooms/:room_id/pins`
pub async fn index(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let room_id = room.id;
    let messages = c.app().db.read(move |conn| PinnedMessage::messages(conn, room_id)).await.map_err(db_error)?;
    let messages = present(c, move |presenter| presenter.messages(&messages)).await?;
    let response = page::framed_page!(c, StatusCode::OK, |ctx| Pinned { ctx, messages: &messages }).await?;
    let fragments = campfire_views::messages::MessageItem::cached_fragments(&c.app().fragment_cache, &messages);
    Ok(response.with_cached_fragments(fragments))
}

/// `GET /rooms/:room_id/pins/ids`
pub async fn ids(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let room_id = room.id;
    let ids = c.app().db.read(move |conn| PinnedMessage::ids(conn, room_id)).await.map_err(db_error)?;
    Ok(c.render(StatusCode::OK, &format::JSON, serde_json::to_string(&ids).map_err(Error::internal)?))
}

/// `POST /messages/:message_id/pin`
pub async fn create(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (user_id, message_id) = (require_current_user(c)?.id, message_id(c)?);
    let now = crate::huddles::unix_now();
    let room_id = c.app().db.write(move |tx| PinnedMessage::pin(tx, user_id, message_id, now)).await.map_err(db_error)?.ok_or(Error::NotFound)?;
    broadcast(c, room_id).await?;
    Ok(concerns::head(StatusCode::NO_CONTENT))
}

/// `DELETE /messages/:message_id/pin`
pub async fn destroy(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (user_id, message_id) = (require_current_user(c)?.id, message_id(c)?);
    let room_id = c.app().db.write(move |tx| PinnedMessage::unpin(tx, user_id, message_id)).await.map_err(db_error)?.ok_or(Error::NotFound)?;
    broadcast(c, room_id).await?;
    Ok(concerns::head(StatusCode::NO_CONTENT))
}

/// Everyone in the room sees the change (lib/workspace/pins.js reads the header's signal).
async fn broadcast(c: &Ctx, room_id: i64) -> Result<()> {
    let (room, ids) = c
        .app()
        .db
        .read(move |conn| Ok((campfire_db::Room::find(conn, room_id)?, PinnedMessage::ids(conn, room_id)?)))
        .await
        .map_err(db_error)?;
    c.app().broadcasts.pins_change(&room, &ids);
    Ok(())
}

fn message_id(c: &Ctx) -> Result<i64> {
    c.param_str("message_id").and_then(cast_integer).ok_or(Error::NotFound)
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};

    use crate::controllers::presenters::test_support::*;

    #[tokio::test]
    async fn a_channels_messages_are_pinned_listed_and_unpinned() {
        let Some(app) = TestApp::boot().await else { return };
        let mut david = app.david();
        let message_id: i64 = app.db().read(|conn| Ok(campfire_db::Message::last_page(conn, ALL_TALK)?.last().unwrap().id)).await.unwrap();

        let pinned = david.write(Req::new(Method::POST, &format!("/messages/{message_id}/pin"))).await;
        assert_eq!(pinned.status, StatusCode::NO_CONTENT, "{}", pinned.text());
        assert_eq!(david.get(&format!("/rooms/{ALL_TALK}/pins/ids")).await.text(), format!("[{message_id}]"));
        let page = david.get(&format!("/rooms/{ALL_TALK}/pins")).await;
        assert!(page.text().contains(&format!(r#"data-message-id="{message_id}""#)), "{}", page.text());

        assert_eq!(david.write(Req::new(Method::POST, "/messages/999999999/pin")).await.status, StatusCode::NOT_FOUND);
        assert_eq!(david.write(Req::new(Method::DELETE, &format!("/messages/{message_id}/pin"))).await.status, StatusCode::NO_CONTENT);
        assert_eq!(david.get(&format!("/rooms/{ALL_TALK}/pins/ids")).await.text(), "[]");
    }
}
