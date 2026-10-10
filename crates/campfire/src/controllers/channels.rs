//! "Browse channels" (plans/redesign.md), this app's own: the channels you're in, with Slack's
//! join and leave. Leaving sets your involvement to "invisible" (out of the sidebar, no
//! notifications), as the notifications menu's "Off and hidden" does; joining sets it back to
//! "mentions", a new membership's default.
use campfire_db::{Involvement, Room, RoomType};
use campfire_kit::{Ctx, Error, Result, StatusCode};
use campfire_views::rooms::{Browse, BrowsedChannel};

use crate::app::AppCtx;
use crate::concerns::{self, Before, before_actions, require_current_user};
use crate::controllers::presenters::page::{self, db_error};
use crate::controllers::rooms::{ensure_permission_to_create_rooms, render_shared_room};

pub async fn index(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let user_id = require_current_user(c)?.id;
    let channels: Vec<BrowsedChannel> = c
        .app()
        .db
        .read(move |conn| Room::browse_for(conn, user_id))
        .await
        .map_err(db_error)?
        .into_iter()
        .map(|browsed| BrowsedChannel {
            id: browsed.room.id,
            name: browsed.room.name.clone().unwrap_or_default(),
            topic: browsed.room.topic.clone(),
            private: browsed.room.room_type == RoomType::Closed,
            members: browsed.member_count,
            joined: browsed.involvement != Some(Involvement::Invisible),
        })
        .collect();
    let can_create = ensure_permission_to_create_rooms(c).await.is_ok();
    page::framed_page!(c, StatusCode::OK, |ctx| Browse { ctx, channels: channels.clone(), can_create }).await
}

/// `POST /channels/:room_id/membership`
pub async fn join(c: &mut Ctx) -> Result {
    set_involvement(c, Involvement::Mentions).await
}

/// `DELETE /channels/:room_id/membership`
pub async fn leave(c: &mut Ctx) -> Result {
    set_involvement(c, Involvement::Invisible).await
}

async fn set_involvement(c: &mut Ctx, involvement: Involvement) -> Result {
    before_actions(c, Before::default()).await?;
    let (membership, room) = concerns::set_room(c).await?;
    if room.room_type == RoomType::Direct {
        return Err(Error::NotFound);
    }
    let previous = membership.involvement;
    let membership = c
        .app()
        .db
        .write(move |tx| {
            let mut membership = membership;
            membership.update_involvement(tx, involvement)?;
            Ok(membership)
        })
        .await
        .map_err(db_error)?;
    // As Rooms::InvolvementsController#update does: the sidebar follows.
    let partials = render_shared_room(c, &room).await?;
    c.app().broadcasts.involvement_change(&room, &membership, previous, &partials).map_err(Error::internal)?;
    let url = c.url_for(&campfire_routes::channels());
    c.redirect_to(&url)
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use campfire_db::{Involvement, Membership};

    use crate::controllers::presenters::test_support::*;

    #[tokio::test]
    async fn channels_are_listed_left_and_joined_again() {
        let Some(app) = TestApp::boot().await else { return };
        let mut david = app.david();
        assert_eq!(david.write(Req::new(Method::DELETE, &format!("/channels/{DIRECT_DAVID_JASON}/membership"))).await.status, StatusCode::NOT_FOUND, "not for direct rooms");
        let page = david.get("/channels").await;
        assert_eq!(page.status, StatusCode::OK);
        assert!(page.text().contains("Browse channels") && page.text().contains(">Leave<"), "{}", page.text());

        let involvement = |app: &TestApp| {
            let db = app.db().clone();
            async move { db.read(|conn| Membership::find_by_room_and_user(conn, ALL_TALK, DAVID)).await.unwrap().and_then(|m| m.involvement) }
        };
        let left = david.write(Req::new(Method::DELETE, &format!("/channels/{ALL_TALK}/membership"))).await;
        assert_eq!(left.location(), Some("http://campfire.test/channels"));
        assert_eq!(involvement(&app).await, Some(Involvement::Invisible));
        assert!(david.get("/channels").await.text().contains(">Join<"));

        david.write(Req::new(Method::POST, &format!("/channels/{ALL_TALK}/membership"))).await;
        assert_eq!(involvement(&app).await, Some(Involvement::Mentions));
    }
}
