//! "People" (plans/redesign.md), this app's own: everyone in the workspace.
use campfire_kit::{Ctx, Result, StatusCode};
use campfire_views::searches::People;

use crate::concerns::{Before, before_actions};
use crate::controllers::presenters::page;

pub async fn index(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let users = crate::controllers::rooms::opens::active_users(c).await?;
    page::framed_page!(c, StatusCode::OK, |ctx| People { ctx, users: &users }).await
}

#[cfg(test)]
mod tests {
    use crate::controllers::presenters::test_support::*;

    #[tokio::test]
    async fn everyone_is_listed() {
        let Some(app) = TestApp::boot().await else { return };
        let page = app.david().get("/people").await;
        assert_eq!(page.status, axum::http::StatusCode::OK);
        assert!(page.text().contains("<title>People</title>") && page.text().contains(&format!("/users/{DAVID}")), "{}", page.text());
    }
}
