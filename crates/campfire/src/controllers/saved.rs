//! Saved for later (plans/redesign.md), this app's own: save a message from its "⋯" menu
//! (lib/workspace/saved.js), see them all at `/saved`.
use campfire_db::SavedMessage;
use campfire_kit::{Ctx, Error, Result, StatusCode, format};
use campfire_views::searches::Saved;

use crate::app::AppCtx;
use crate::concerns::{Before, before_actions, cast_integer, require_current_user};
use crate::controllers::messages::present;
use crate::controllers::presenters::page::{self, db_error};

pub async fn index(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let user_id = require_current_user(c)?.id;
    let messages = c.app().db.read(move |conn| SavedMessage::messages(conn, user_id)).await.map_err(db_error)?;
    let messages = present(c, move |presenter| presenter.messages(&messages)).await?;
    let response = page::framed_page!(c, StatusCode::OK, |ctx| Saved { ctx, messages: &messages }).await?;
    let fragments = campfire_views::messages::MessageItem::cached_fragments(&c.app().fragment_cache, &messages);
    Ok(response.with_cached_fragments(fragments))
}

/// `GET /saved/ids`: the ids of yours, for the "⋯" menu's "Save for later" / "Remove from saved".
pub async fn ids(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let user_id = require_current_user(c)?.id;
    let ids = c.app().db.read(move |conn| SavedMessage::ids(conn, user_id)).await.map_err(db_error)?;
    Ok(c.render(StatusCode::OK, &format::JSON, serde_json::to_string(&ids).map_err(Error::internal)?))
}

/// `POST /messages/:message_id/save`
pub async fn create(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (user_id, message_id) = (require_current_user(c)?.id, message_id(c)?);
    let now = crate::huddles::unix_now();
    let saved = c.app().db.write(move |tx| SavedMessage::save(tx, user_id, message_id, now)).await.map_err(db_error)?;
    if !saved {
        return Err(Error::NotFound);
    }
    Ok(crate::concerns::head(StatusCode::NO_CONTENT))
}

/// `DELETE /messages/:message_id/save`
pub async fn destroy(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (user_id, message_id) = (require_current_user(c)?.id, message_id(c)?);
    c.app().db.write(move |tx| SavedMessage::unsave(tx, user_id, message_id)).await.map_err(db_error)?;
    Ok(crate::concerns::head(StatusCode::NO_CONTENT))
}

fn message_id(c: &Ctx) -> Result<i64> {
    c.param_str("message_id").and_then(cast_integer).ok_or(Error::NotFound)
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};

    use crate::controllers::presenters::test_support::*;

    #[tokio::test]
    async fn messages_are_saved_listed_and_removed() {
        let Some(app) = TestApp::boot().await else { return };
        let mut david = app.david();
        let message_id: i64 = app
            .db()
            .read(|conn| Ok(campfire_db::Message::last_page(conn, ALL_TALK)?.last().unwrap().id))
            .await
            .unwrap();
        assert_eq!(david.get("/saved").await.text().contains("Nothing saved yet"), true);

        let saved = david.write(Req::new(Method::POST, &format!("/messages/{message_id}/save"))).await;
        assert_eq!(saved.status, StatusCode::NO_CONTENT, "{}", saved.text());
        assert_eq!(david.get("/saved/ids").await.text(), format!("[{message_id}]"));
        let page = david.get("/saved").await;
        assert!(page.text().contains(&format!(r#"data-message-id="{message_id}""#)) && page.text().contains("1 message"), "{}", page.text());

        assert_eq!(david.write(Req::new(Method::POST, "/messages/999999999/save")).await.status, StatusCode::NOT_FOUND);
        assert_eq!(david.write(Req::new(Method::DELETE, &format!("/messages/{message_id}/save"))).await.status, StatusCode::NO_CONTENT);
        assert_eq!(david.get("/saved/ids").await.text(), "[]");
    }
}
