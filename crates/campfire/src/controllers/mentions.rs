//! "Mentions" (plans/redesign.md), this app's own: the latest messages that @mention you.
use campfire_db::Message;
use campfire_kit::{Ctx, Result, StatusCode};
use campfire_views::searches::Mentions;

use crate::app::AppCtx;
use crate::concerns::{Before, before_actions, require_current_user};
use crate::controllers::messages::present;
use crate::controllers::presenters::page::{self, db_error};

/// How many it lists.
const LIMIT: usize = 50;

pub async fn index(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let user_id = require_current_user(c)?.id;
    let app = c.app().clone();
    let messages = c.app().db.read(move |conn| Message::mentioning(conn, &*app.db.env().rich_text, user_id, LIMIT)).await.map_err(db_error)?;
    let messages = present(c, move |presenter| presenter.messages(&messages)).await?;
    let response = page::framed_page!(c, StatusCode::OK, |ctx| Mentions { ctx, messages: &messages }).await?;
    let fragments = campfire_views::messages::MessageItem::cached_fragments(&c.app().fragment_cache, &messages);
    Ok(response.with_cached_fragments(fragments))
}

#[cfg(test)]
mod tests {
    use crate::controllers::presenters::test_support::*;

    #[tokio::test]
    async fn mentions_are_listed() {
        let Some(app) = TestApp::boot().await else { return };
        let page = app.david().get("/mentions").await;
        assert_eq!(page.status, axum::http::StatusCode::OK);
        assert!(page.text().contains("<title>Mentions</title>") && page.text().contains("ws-search__summary"), "{}", page.text());
    }
}
