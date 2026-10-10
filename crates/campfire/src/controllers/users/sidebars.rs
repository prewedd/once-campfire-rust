//! `Users::SidebarsController` (reference/app/controllers/users/sidebars_controller.rb): the room
//! list, loaded into the `user_sidebar` turbo frame.

use campfire_db::Account;
use campfire_kit::{Ctx, Error, Result, StatusCode, format};
use campfire_views::users;

use crate::app::AppCtx;
use crate::channels::user_gid;
use crate::concerns::{self, Before};
use crate::controllers::presenters::{self, view_context};

pub async fn show(c: &mut Ctx) -> Result {
    concerns::before_actions(c, Before::default()).await?;
    c.respond_to(&[&format::HTML])?;
    if let Some(response) = crate::response_cache::lookup(c) {
        return Ok(response);
    }
    let user = concerns::require_current_user(c)?.clone();
    let secrets = c.app().secrets.clone();
    let (sidebar, restricted) = {
        let (user, secrets, fragments) = (user.clone(), secrets.clone(), crate::response_cache::fragments(c));
        c.app()
            .read(move |conn| {
                // The direct rooms' fragments come from the store the render then uses.
                let sidebar = campfire_views::fragment_cache::with(&fragments, || presenters::accounts::sidebar(conn, &secrets, &user))?;
                let restricted = Account::first(conn)?.is_some_and(|account| account.settings().restrict_room_creation_to_administrators());
                Ok((sidebar, restricted))
            })
            .await?
    };

    let data = SidebarData {
        current_user: presenters::user_summary(&secrets, &user),
        // turbo_stream_from :rooms / turbo_stream_from Current.user, :rooms
        rooms_stream: rails_compat::turbo::signed_stream_name(&secrets, &["rooms"]),
        user_rooms_stream: rails_compat::turbo::signed_stream_name(&secrets, &[&user_gid(user.id).to_param(), "rooms"]),
        sidebar,
        // `Current.user.administrator? || !Current.account.settings.restrict_room_creation_to_administrators?`
        can_create_rooms: user.is_administrator() || !restricted,
    };
    view_context::page_or_frame(
        c,
        StatusCode::OK,
        |ctx| campfire_views::render_sized!(data.page(ctx)),
        |ctx| {
            let page = data.page(ctx);
            campfire_views::layouts::frame(ctx, page.as_head(), page.as_content())
        },
    )
    .await
}

struct SidebarData {
    current_user: campfire_views::users::UserSummary,
    rooms_stream: String,
    user_rooms_stream: String,
    sidebar: presenters::accounts::Sidebar,
    can_create_rooms: bool,
}

impl SidebarData {
    fn page<'a>(&self, ctx: &'a campfire_views::ViewContext<'a>) -> users::SidebarShow<'a> {
        users::SidebarShow {
            ctx,
            current_user: self.current_user.clone(),
            rooms_stream: self.rooms_stream.clone(),
            user_rooms_stream: self.user_rooms_stream.clone(),
            direct_memberships: self.sidebar.direct_memberships.clone(),
            direct_placeholder_users: self.sidebar.direct_placeholder_users.clone(),
            other_memberships: self.sidebar.other_memberships.clone(),
            can_create_rooms: self.can_create_rooms,
        }
    }
}

/// `POST /users/me/sidebar/positions`: the channels in the order this person dragged them into,
/// as `room_ids=3,1,7` (plans/redesign.md). Rooms they aren't in are ignored.
pub async fn reorder(c: &mut Ctx) -> Result {
    concerns::before_actions(c, Before::default()).await?;
    let user_id = concerns::require_current_user(c)?.id;
    let room_ids: Vec<i64> = c.param_str("room_ids").unwrap_or_default().split(',').filter_map(|id| id.trim().parse().ok()).take(500).collect();
    c.app()
        .db
        .write(move |tx| campfire_db::Membership::reorder(tx, user_id, &room_ids))
        .await
        .map_err(Error::internal)?;
    Ok(concerns::head(StatusCode::NO_CONTENT))
}

/// `GET /users/me/sidebar/unread_counts`: unread messages per unread direct room, as
/// `{"room id": count}`, for the sidebar's red badges (plans/redesign.md).
pub async fn unread_counts(c: &mut Ctx) -> Result {
    concerns::before_actions(c, Before::default()).await?;
    let user_id = concerns::require_current_user(c)?.id;
    let counts = c.app().db.read(move |conn| campfire_db::Membership::unread_direct_counts(conn, user_id)).await.map_err(Error::internal)?;
    let json: serde_json::Map<String, serde_json::Value> = counts.into_iter().map(|(room, count)| (room.to_string(), count.into())).collect();
    Ok(c.render(StatusCode::OK, &format::JSON, serde_json::Value::Object(json).to_string()))
}

#[cfg(test)]
mod unread_counts_tests {
    use crate::controllers::presenters::test_support::*;

    #[tokio::test]
    async fn unread_counts_are_json_by_room() {
        let Some(app) = TestApp::boot().await else { return };
        let mut david = app.david();
        let reply = david.get("/users/me/sidebar/unread_counts").await;
        assert_eq!(reply.status, axum::http::StatusCode::OK);
        let counts: serde_json::Value = serde_json::from_str(&reply.text()).unwrap();
        assert!(counts.is_object(), "{counts}");
    }
}
