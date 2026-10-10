//! Custom statuses (plans/redesign.md), this app's own: set and cleared from your profile, and
//! read by every page as JSON (lib/workspace/status.js), which shows them by people's names.
use campfire_db::UserStatus;
use campfire_kit::{Ctx, Error, Result, StatusCode, format};
use serde_json::{Map, Value, json};

use crate::app::AppCtx;
use crate::concerns::{self, Before};

/// A status clears itself at most this far ahead.
const LONGEST_MINUTES: i64 = 60 * 24 * 31;

fn now() -> i64 {
    crate::huddles::unix_now()
}

/// `GET /users/statuses.json`: everyone's current status, by user id.
pub async fn index(c: &mut Ctx) -> Result {
    concerns::before_actions(c, Before::default()).await?;
    concerns::require_current_user(c)?;
    let statuses = c.app().db.read(|conn| UserStatus::current(conn, now())).await.map_err(Error::internal)?;
    let by_user: Map<String, Value> =
        statuses.into_iter().map(|status| (status.user_id.to_string(), json!({ "emoji": status.emoji, "text": status.text, "clears_at": status.clears_at }))).collect();
    Ok(c.render(StatusCode::OK, &format::JSON, Value::Object(by_user).to_string()))
}

/// `PUT /users/me/status`: `status[emoji]`, `status[text]` and `status[clear_in]` (minutes; blank
/// keeps it until cleared). Back to the profile, where it's set.
pub async fn update(c: &mut Ctx) -> Result {
    concerns::before_actions(c, Before::default()).await?;
    let user_id = concerns::require_current_user(c)?.id;
    let params = c.params.require("status")?.permit(&campfire_kit::permit_keys(&["emoji", "text", "clear_in"]));
    let value = |key: &str| params.get(key).and_then(|value| value.as_str().map(str::to_string)).unwrap_or_default();
    let (emoji, text) = (value("emoji"), value("text"));
    let now = now();
    let clears_at = value("clear_in").trim().parse::<i64>().ok().filter(|minutes| *minutes > 0).map(|minutes| now + minutes.min(LONGEST_MINUTES) * 60);
    c.app().db.write(move |tx| UserStatus::set(tx, user_id, &emoji, &text, clears_at, now)).await.map_err(Error::internal)?;
    c.app().broadcasts.statuses_change(now);
    back_to_profile(c)
}

/// `DELETE /users/me/status`
pub async fn destroy(c: &mut Ctx) -> Result {
    concerns::before_actions(c, Before::default()).await?;
    let user_id = concerns::require_current_user(c)?.id;
    c.app().db.write(move |tx| UserStatus::clear(tx, user_id)).await.map_err(Error::internal)?;
    c.app().broadcasts.statuses_change(now());
    back_to_profile(c)
}

/// `PUT /users/me/notifications_pause`: `minutes` (at most a week) of no pushes (plans/redesign.md).
pub async fn pause(c: &mut Ctx) -> Result {
    concerns::before_actions(c, Before::default()).await?;
    let user_id = concerns::require_current_user(c)?.id;
    let minutes = c.param_str("minutes").and_then(|value| value.trim().parse::<i64>().ok()).filter(|minutes| *minutes > 0).ok_or(Error::NotFound)?;
    let until = now() + minutes.min(7 * 24 * 60) * 60;
    c.app().db.write(move |tx| campfire_db::NotificationPause::pause(tx, user_id, until)).await.map_err(Error::internal)?;
    back_to_profile(c)
}

/// `DELETE /users/me/notifications_pause`
pub async fn resume(c: &mut Ctx) -> Result {
    concerns::before_actions(c, Before::default()).await?;
    let user_id = concerns::require_current_user(c)?.id;
    c.app().db.write(move |tx| campfire_db::NotificationPause::resume(tx, user_id)).await.map_err(Error::internal)?;
    back_to_profile(c)
}

fn back_to_profile(c: &mut Ctx) -> Result {
    let url = c.url_for(&campfire_routes::user_profile());
    c.redirect_to(&url)
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};

    use crate::controllers::presenters::test_support::*;

    #[tokio::test]
    async fn a_status_is_set_read_by_everyone_and_cleared() {
        let Some(app) = TestApp::boot().await else { return };
        let mut david = app.david();
        let set = david.write(Req::new(Method::PUT, "/users/me/status").form(&[("status[emoji]", "📅"), ("status[text]", " In a meeting "), ("status[clear_in]", "60")])).await;
        assert_eq!(set.status, StatusCode::FOUND, "{}", set.text());
        assert_eq!(set.location(), Some("http://campfire.test/users/me/profile"));

        assert_ne!(app.anonymous().get("/users/statuses.json").await.status, StatusCode::OK, "only for people signed in");
        let statuses: serde_json::Value = serde_json::from_str(&david.get("/users/statuses.json").await.text()).unwrap();
        let mine = &statuses[DAVID.to_string()];
        assert_eq!((mine["emoji"].as_str(), mine["text"].as_str()), (Some("📅"), Some("In a meeting")));
        assert!(mine["clears_at"].as_i64().unwrap() > super::now() + 59 * 60, "clears in an hour");
        assert!(david.get("/users/me/profile").await.text().contains(r#"value="In a meeting""#));

        let cleared = david.write(Req::new(Method::DELETE, "/users/me/status")).await;
        assert_eq!(cleared.status, StatusCode::FOUND);
        assert_eq!(david.get("/users/statuses.json").await.text(), "{}");
    }
}
