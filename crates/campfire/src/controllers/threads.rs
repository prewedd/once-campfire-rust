//! Threads (plans/threads.md): the thread view, and the pages and refreshes its messages
//! controller asks for, which work like a room's over the thread's parent and replies.

use askama::Template;
use campfire_db::{Message, Room, Timeline};
use campfire_kit::{Ctx, Error, Freshness, Result, StatusCode, format};
use campfire_views::threads as views;

use crate::app::AppCtx;
use crate::concerns::{self, Before, before_actions, cast_integer, require_current_user};
use crate::controllers::messages::find_paged;
use crate::controllers::presenters::page::{self, db_error};
use crate::controllers::presenters::{Presenter, cache_key_with_version, user_view};
use crate::controllers::rooms::refreshes::set_last_updated_at;

/// `GET /rooms/:room_id/threads/:id`, and `/rooms/:room_id/threads/:id/@:message_id` for the
/// thread around one of its replies. A reply's own id redirects to its thread.
pub async fn show(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let message = set_thread_message(c, &room, "id").await?;
    if let Some(parent_id) = message.thread_parent_id {
        let url = c.url_for(&campfire_routes::room_thread_at_message(room.id, parent_id, message.id));
        return c.redirect_to(&url);
    }
    concerns::remember_last_room_visited(c, room.id);
    let around = c.param_str("message_id").and_then(cast_integer);
    render_show(c, room, message, around).await
}

/// `GET /rooms/:room_id/threads/:thread_id/messages`: a page of the thread before or after a
/// message, or its last page (`messages#index` for a thread); 204 when empty.
pub async fn messages(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let parent = set_thread_message(c, &room, "thread_id").await?;
    let messages = find_paged(c, room.id, Timeline::Thread(parent.thread_root_id())).await?;
    if messages.is_empty() {
        return Ok(c.head(StatusCode::NO_CONTENT));
    }
    let etag = messages.iter().map(|m| cache_key_with_version("messages", m.id, m.updated_at.jiff())).collect::<Vec<_>>().join("/");
    let freshness = Freshness {
        etag: Some(etag),
        last_modified: messages.iter().map(|m| m.updated_at.jiff()).max(),
        template: Some("threads/messages".into()),
        ..Freshness::default()
    };
    if let Some(not_modified) = c.fresh_when(freshness) {
        return Ok(not_modified);
    }
    c.respond_to(&[&format::HTML])?;
    let items = crate::controllers::messages::present(c, move |presenter| presenter.messages(&messages)).await?;
    let response =
        page::bare(c, StatusCode::OK, &format::HTML, |ctx| campfire_views::messages::Index { ctx, messages: &items }.render()).await?;
    let fragments = campfire_views::messages::MessageItem::cached_fragments(&c.app().fragment_cache, &items);
    Ok(response.with_cached_fragments(fragments))
}

/// `GET /rooms/:room_id/threads/:thread_id/refresh`: what changed in the thread since the client
/// last loaded it (`rooms/refreshes#show` for a thread).
pub async fn refresh(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let parent = set_thread_message(c, &room, "thread_id").await?;
    let last_updated_at = set_last_updated_at(c)?;
    c.respond_to(&[&format::TURBO_STREAM])?;

    let app = c.app().clone();
    let request_host = Some(c.request.host());
    let parent_id = parent.thread_root_id();
    let refresh = c
        .app()
        .db
        .read(move |conn| {
            let thread = Timeline::Thread(parent_id);
            let new_messages = thread.page_created_since(conn, last_updated_at)?;
            let new_ids: Vec<i64> = new_messages.iter().map(|message| message.id).collect();
            let updated_messages = thread.page_updated_since(conn, last_updated_at, &new_ids)?;
            let presenter = Presenter::new(conn, &app, request_host);
            campfire_views::fragment_cache::with(&app.fragment_cache, || {
                Ok(views::RefreshView {
                    parent_id,
                    new_messages: presenter.messages(&new_messages)?,
                    updated_messages: presenter.messages(&updated_messages)?,
                })
            })
        })
        .await
        .map_err(db_error)?;
    page::bare(c, StatusCode::OK, &format::TURBO_STREAM, |ctx| views::RefreshShow { ctx, refresh: &refresh }.render()).await
}

/// The message named by the `param` path parameter, in the room.
async fn set_thread_message(c: &mut Ctx, room: &Room, param: &str) -> Result<Message> {
    let Some(id) = c.param_str(param).and_then(cast_integer) else {
        return Err(Error::NotFound);
    };
    let room_id = room.id;
    c.app().db.read(move |conn| Message::find_in_room(conn, room_id, id)).await.map_err(db_error)
}

/// The thread view: the page around `around` when it's in the thread, else the last page.
async fn render_show(c: &mut Ctx, room: Room, parent: Message, around: Option<i64>) -> Result {
    let app = c.app().clone();
    let user = require_current_user(c)?.clone();
    let request_host = Some(c.request.host());
    let show = c
        .app()
        .db
        .read(move |conn| {
            let thread = Timeline::Thread(parent.id);
            let anchor = around.map(|id| Message::find_by_id(conn, id)).transpose()?.flatten();
            let messages = match anchor {
                Some(message) if message.thread_root_id() == parent.id && message.room_id == room.id => {
                    thread.page_around(conn, &message)?
                }
                _ => thread.last_page(conn)?,
            };
            let presenter = Presenter::new(conn, &app, request_host);
            let room_gid = crate::channels::room_gid(&room).to_param();
            Ok(views::ShowView {
                room: presenter.room_view(&room, &user)?,
                parent_id: parent.id,
                updated_at: parent.updated_at.jiff(),
                user: user_view(&app.secrets, &user),
                messages: campfire_views::fragment_cache::with(&app.fragment_cache, || presenter.messages(&messages))?,
                messages_stream_name: rails_compat::turbo::signed_stream_name(&app.secrets, &[&room_gid, "messages"]),
            })
        })
        .await
        .map_err(db_error)?;
    let response = if c.turbo_frame_request_id() == Some(views::PANE_FRAME) {
        page::bare(c, StatusCode::OK, &format::HTML, |ctx| views::Pane { ctx, show: &show }.render()).await?
    } else {
        page::framed_page!(c, StatusCode::OK, |ctx| views::Show { ctx, show: &show }).await?
    };
    let fragments = campfire_views::messages::MessageItem::cached_fragments(&c.app().fragment_cache, &show.messages);
    Ok(response.with_cached_fragments(fragments))
}

#[cfg(test)]
mod tests;
