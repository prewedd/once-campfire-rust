//! `MessagesController` (reference/app/controllers/messages_controller.rb), including the
//! multipart attachment upload the composer's `FileUploader` posts here, plus what
//! `Messages::ByBotsController` reuses: creating a message (with `process_attachment`),
//! delivering webhooks and the broadcasts.

pub mod boosts;
pub mod by_bots;

use askama::Template;
use campfire_db::{Message, NewMessage, Role, Room, Status, Timeline, User};
use campfire_kit::format;
use campfire_kit::{Ctx, Error, Freshness, Param, Result, StatusCode, halt, permit_keys};
use campfire_richtext::Content;
use campfire_storage::{Blob, Staged, Variation};
use campfire_views::messages as views;

use crate::active_storage::{self, keep_after_commit};
use crate::app::{App, AppCtx};
use crate::concerns::{self, Before, before_actions, cast_integer, require_current_user};
use crate::controllers::presenters::page::{self, Rendered, db_error};
use crate::controllers::presenters::attachments::Assignment;
use crate::controllers::presenters::{DbResolver, Presenter, cache_key_with_version, room_kind, storage_error};

// --- Actions ------------------------------------------------------------------------------------

/// `index` (`layout false`): the page before/after a message, or the last page; 204 when empty.
pub async fn index(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let messages = find_paged_messages(c, &room).await?;
    if messages.is_empty() {
        return Ok(c.head(StatusCode::NO_CONTENT));
    }
    // fresh_when @messages: the records' cache keys, their latest updated_at, and the template.
    let etag = messages.iter().map(|m| cache_key_with_version("messages", m.id, m.updated_at.jiff())).collect::<Vec<_>>().join("/");
    let freshness = Freshness {
        etag: Some(etag),
        last_modified: messages.iter().map(|m| m.updated_at.jiff()).max(),
        template: Some(TEMPLATE_DIGEST_INDEX.into()),
        ..Freshness::default()
    };
    if let Some(not_modified) = c.fresh_when(freshness) {
        return Ok(not_modified);
    }
    c.respond_to(&[&format::HTML])?;
    let views = present(c, move |presenter| presenter.messages(&messages)).await?;
    let response = page::bare(c, StatusCode::OK, &format::HTML, |ctx| views::Index { ctx, messages: &views }.render()).await?;
    let fragments = campfire_views::messages::MessageItem::cached_fragments(&c.app().fragment_cache, &views);
    Ok(response.with_cached_fragments(fragments))
}

/// Stands in for the digest `ETagWithTemplateDigest` adds for `messages/index` (only the ETag's
/// shape has to match the reference).
const TEMPLATE_DIGEST_INDEX: &str = "messages/index";

/// `create`: `set_room` runs inside the action, and a room that's gone renders `room_not_found`.
pub async fn create(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let room = match concerns::set_room(c).await {
        Ok((_, room)) => room,
        Err(Error::NotFound) => return render_room_not_found(c).await,
        Err(error) => return Err(error),
    };
    let attributes = message_params(c)?;
    let message = create_message(c, &room, attributes).await?;
    broadcast_create(c, &room, &message).await?;
    deliver_webhooks_to_bots(c, &room, &message).await?;

    // The message partial comes out of the fragment cache `broadcast_create` just filled
    // (`cache [ message, "presentation-v3" ]`), so it's the request-less rendering: no CSRF
    // tokens in its forms.
    c.respond_to(&[&format::TURBO_STREAM])?;
    let kind = room_kind(room.room_type);
    let app = c.app().clone();
    let base_url = c.url_for("");
    let html = c
        .app()
        .db
        .read(move |conn| {
            let presenter = Presenter::new(conn, &app, None);
            let item = campfire_views::fragment_cache::with(&app.fragment_cache, || presenter.message_item(&message))?;
            let account = campfire_db::Account::first(conn)?;
            page::render_detached_at(&app, account.as_ref(), &base_url, |ctx| match message.thread_parent_id {
                None => views::CreateStream { ctx, message: &item, room_kind: kind }.render(),
                Some(parent_id) => campfire_views::threads::CreateStream { ctx, message: &item, parent_id }.render(),
            })
            .map_err(|e| campfire_db::Error::Other(e.to_string()))
        })
        .await
        .map_err(db_error)?;
    Ok(c.render(StatusCode::OK, &format::TURBO_STREAM, html))
}

pub async fn show(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let message = set_message(c, &room).await?;
    c.respond_to(&[&format::HTML])?;
    let view = present(c, move |presenter| presenter.message(&message)).await?;
    page::content_in_application_layout(c, StatusCode::OK, |ctx| views::Show { ctx, message: &view }.render()).await
}

pub async fn edit(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let message = set_message(c, &room).await?;
    ensure_can_administer(c, &message)?;
    c.respond_to(&[&format::HTML])?;
    let edit = present(c, move |presenter| {
        Ok(views::EditView { editable_body_html: presenter.editable_body(&message)?, message: presenter.message(&message)? })
    })
    .await?;
    page::content_in_application_layout(c, StatusCode::OK, |ctx| views::Edit { ctx, edit: &edit }.render()).await
}

pub async fn update(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let message = set_message(c, &room).await?;
    ensure_can_administer(c, &message)?;
    let attributes = message_params(c)?;
    let message = update_message(c, message, attributes).await?;
    broadcast_replace(c, &room, &message).await?;

    // respond_to html: redirect; json: `render :show`, which has no JSON template here.
    match c.respond_to(&[&format::HTML, &format::JSON])? {
        f if *f == format::JSON => Err(Error::internal(anyhow::anyhow!("Missing template messages/show"))),
        _ => {
            let url = c.url_for(&campfire_routes::room_message(room.id, message.id));
            c.redirect_to(&url)
        }
    }
}

pub async fn destroy(c: &mut Ctx) -> Result {
    before_actions(c, Before::default()).await?;
    let (_, room) = concerns::set_room(c).await?;
    let message = set_message(c, &room).await?;
    ensure_can_administer(c, &message)?;
    destroy_message(c, &room, &message).await?;

    c.respond_to(&[&format::TURBO_STREAM])?;
    let view = present(c, move |presenter| presenter.message(&message)).await?;
    page::bare(c, StatusCode::OK, &format::TURBO_STREAM, |_| views::DestroyStream { message: &view }.render()).await
}

// --- Before-actions and params --------------------------------------------------------------------

/// `@room.messages.find(params[:id])`
pub(crate) async fn set_message(c: &mut Ctx, room: &Room) -> Result<Message> {
    let Some(id) = c.param_str("id").and_then(cast_integer) else { return Err(Error::NotFound) };
    let room_id = room.id;
    c.app().db.read(move |conn| Message::find_in_room(conn, room_id, id)).await.map_err(db_error)
}

/// `head :forbidden unless Current.user.can_administer?(@message)`
pub(crate) fn ensure_can_administer(c: &mut Ctx, message: &Message) -> Result<()> {
    if !require_current_user(c)?.can_administer(Some(message.creator_id), false) {
        return halt(concerns::head(StatusCode::FORBIDDEN));
    }
    Ok(())
}

/// What `create_with_attachment!`/`update!` receive.
#[derive(Debug, Default, Clone)]
pub(crate) struct MessageParams {
    pub body: Option<String>,
    /// `attachment=`: `None` when the key wasn't given.
    pub attachment: Option<Assignment>,
    pub client_message_id: Option<String>,
    /// The message whose thread this is a reply in (plans/threads.md).
    pub thread_parent_id: Option<i64>,
}

/// `params.require(:message).permit(:body, :attachment, :client_message_id)`, plus the thread
/// a reply goes in: `message[thread_parent_id]`, or `thread_parent_id` in the query, which is
/// how the thread composer's uploads (posted without the form's fields) carry it.
fn message_params(c: &Ctx) -> Result<MessageParams> {
    let message = c.params.require("message")?;
    let permitted = message.permit(&permit_keys(&["body", "attachment", "client_message_id", "thread_parent_id"]));
    let text = |key: &str| permitted.get(key).and_then(Param::as_str).map(str::to_string);
    Ok(MessageParams {
        body: text("body"),
        attachment: attachment_assignment(&permitted)?,
        client_message_id: text("client_message_id"),
        thread_parent_id: thread_parent_id_param(text("thread_parent_id").or_else(|| c.param_str("thread_parent_id").map(str::to_string))),
    })
}

/// A blank thread id is no thread; anything else must be an id.
pub(crate) fn thread_parent_id_param(value: Option<String>) -> Option<i64> {
    value.as_deref().map(str::trim).filter(|value| !value.is_empty()).and_then(cast_integer)
}

/// What assigning the permitted `attachment` does: an upload replaces the attachment, nil or ""
/// removes it (`Attached::Changes::DeleteOne`), anything else raises.
pub(crate) fn attachment_assignment(permitted: &campfire_kit::ParamMap) -> Result<Option<Assignment>> {
    match Assignment::from_params(permitted, "attachment")? {
        Assignment::Unchanged => Ok(None),
        assignment => Ok(Some(assignment)),
    }
}

/// `@room.messages.find(params[:before])` and friends (`find_paged_messages`).
pub(crate) async fn find_paged_messages(c: &Ctx, room: &Room) -> Result<Vec<Message>> {
    find_paged(c, room.id, Timeline::Room(room.id)).await
}

/// The page of `timeline` before `params[:before]`, after `params[:after]`, or its last page.
/// The anchor can be any message in the room.
pub(crate) async fn find_paged(c: &Ctx, room_id: i64, timeline: Timeline) -> Result<Vec<Message>> {
    let present = |key: &str| c.params.get(key).filter(|p| p.is_present()).map(|p| p.as_str().and_then(cast_integer));
    let (before, after) = (present("before"), present("after"));
    c.app()
        .db
        .read(move |conn| match (before, after) {
            (Some(before), _) => {
                let message = Message::find_in_room(conn, room_id, before.ok_or(campfire_db::Error::RecordNotFound("Message"))?)?;
                timeline.page_before(conn, &message)
            }
            (None, Some(after)) => {
                let message = Message::find_in_room(conn, room_id, after.ok_or(campfire_db::Error::RecordNotFound("Message"))?)?;
                timeline.page_after(conn, &message)
            }
            (None, None) => timeline.last_page(conn),
        })
        .await
        .map_err(db_error)
}

// --- Creating, updating, destroying ---------------------------------------------------------------

/// `@room.messages.create_with_attachment!(attributes)`: the message (with its uploaded blob, in
/// one transaction), then `process_attachment`. The upload's file is copied into storage and the
/// body canonicalized before the transaction, so the writer only inserts rows.
pub(crate) async fn create_message(c: &Ctx, room: &Room, attributes: MessageParams) -> Result<Message> {
    let creator_id = require_current_user(c)?.id;
    let room_id = room.id;
    let attachment = match attributes.attachment {
        Some(Assignment::Create(upload)) => Some(upload.stage(c.app()).await?),
        Some(Assignment::Invalid) => return Err(invalid_attachment()),
        _ => None,
    };
    let body = match attributes.body {
        Some(body) => Some(canonicalize_body(c.app(), body, Some(c.request.host())).await?),
        None => None,
    };
    let requested_parent_id = attributes.thread_parent_id;
    let (message, blob) = c
        .app()
        .db
        .write(move |tx| {
            // Checked in the write, so the parent can't go between the check and the insert.
            let thread_parent_id = requested_parent_id.map(|id| thread_root_id(tx.conn(), room_id, id)).transpose()?;
            let blob = attachment.map(|staged| save_staged(tx, staged)).transpose()?;
            let message = Message::create(
                tx,
                NewMessage {
                    room_id,
                    creator_id,
                    client_message_id: attributes.client_message_id,
                    body,
                    attachment_blob_id: blob.as_ref().map(|blob| blob.id),
                    thread_parent_id,
                },
            )?;
            Ok((message, blob))
        })
        .await
        .map_err(db_error)?;
    if let Some(blob) = blob {
        process_attachment(c.app(), blob).await?;
    }
    let id = message.id;
    c.app().db.read(move |conn| Message::find(conn, id)).await.map_err(db_error)
}

/// The thread a reply to `message_id` goes in: the message's own, or the one it heads (threads
/// are one level deep). It must be in the room, or the reply is a 404.
pub(crate) fn thread_root_id(conn: &campfire_db::Connection, room_id: i64, message_id: i64) -> campfire_db::Result<i64> {
    Ok(Message::find_in_room(conn, room_id, message_id)?.thread_root_id())
}

/// Inserts a staged blob's row, keeping its file once the transaction commits.
pub(crate) fn save_staged(tx: &mut campfire_db::Tx<'_>, staged: Staged) -> campfire_db::Result<Blob> {
    let blob = staged.insert(tx.conn(), tx.now().jiff()).map_err(storage_error)?;
    keep_after_commit(tx, staged);
    Ok(blob)
}

/// [`canonical_body`] on a reader, ahead of the write that stores it.
pub(crate) async fn canonicalize_body(app: &App, body: String, request_host: Option<String>) -> Result<String> {
    let app2 = app.clone();
    app.db.read(move |conn| Ok(canonical_body(conn, &app2, &body, request_host))).await.map_err(db_error)
}

/// A canonical body as plain text, for previews (a scheduled message's).
pub(crate) fn plain_text_of(conn: &campfire_db::Connection, app: &App, body: &str) -> String {
    let resolver = DbResolver { conn, secrets: &app.secrets, now: app.clock.now() };
    let ctx = resolver.render_context(None);
    Content::load(body, &ctx).and_then(|content| content.to_plain_text(&ctx)).unwrap_or_default()
}

/// Assigning a String to a rich text attribute stores the canonicalized content
/// (`ActionText::Content.new(body, canonicalize: true).to_html`).
pub(crate) fn canonical_body(conn: &campfire_db::Connection, app: &App, body: &str, request_host: Option<String>) -> String {
    let resolver = DbResolver { conn, secrets: &app.secrets, now: app.clock.now() };
    let ctx = resolver.render_context(request_host);
    Content::load(body, &ctx).map(|content| content.to_html()).unwrap_or_else(|_| body.to_string())
}

/// Assigning something that isn't an upload, a signed blob id, nil or "".
fn invalid_attachment() -> Error {
    Error::internal(anyhow::anyhow!("Could not find or build blob: expected attachable"))
}

/// `Message#process_attachment`: analyze the blob now (its `after_update` touches the message),
/// then generate the video preview or the `:thumb` representation.
pub(crate) async fn process_attachment(app: &App, blob: Blob) -> Result<()> {
    let blob = analyze_attachment(app, blob).await?;
    if blob.is_video() {
        // attachment.preview(format: :webp).processed
        active_storage::processed_preview(app, blob, Variation::format_only("webp")).await?;
    } else if blob.is_representable() {
        // attachment.representation(:thumb).processed
        let thumb = Variation::resize_to_limit(1200, 800, None);
        active_storage::processed_representation(app, blob, thumb).await?;
    }
    Ok(())
}

/// `blob.analyze`: its `after_update` touches the attached records. The file is analyzed off the
/// writer.
async fn analyze_attachment(app: &App, blob: Blob) -> Result<Blob> {
    let metadata = active_storage::analyzed_metadata(app, &blob).await?;
    app.db
        .write(move |tx| {
            let mut blob = blob;
            blob.update_metadata(tx.conn(), metadata).map_err(storage_error)?;
            touch_attachment_records(tx, blob.id)?;
            Ok(blob)
        })
        .await
        .map_err(db_error)
}

/// `Blob#touch_attachments`: each attached record is touched (a message also touches its room).
fn touch_attachment_records(tx: &mut campfire_db::Tx<'_>, blob_id: i64) -> campfire_db::Result<()> {
    for (record_type, record_id) in campfire_storage::blob::attachment_records(tx.conn(), blob_id).map_err(storage_error)? {
        if record_type == "Message" {
            Message::find(tx.conn(), record_id)?.touch(tx)?;
        }
    }
    Ok(())
}

/// `@message.update!(message_params)`. A new attachment replaces the old one (whose blob is purged
/// later) without `process_attachment`: the blob is only analyzed, by `ActiveStorage::AnalyzeJob`
/// after commit (verified against the reference with a bot's `PUT` and `attachment`).
pub(crate) async fn update_message(c: &Ctx, message: Message, attributes: MessageParams) -> Result<Message> {
    let attachment = match attributes.attachment {
        Some(Assignment::Invalid) => return Err(invalid_attachment()),
        Some(Assignment::Create(upload)) => Some(Some(upload.stage(c.app()).await?)),
        Some(_) => Some(None),
        None => None,
    };
    let body = match attributes.body {
        Some(body) => Some(canonicalize_body(c.app(), body, Some(c.request.host())).await?),
        None => None,
    };
    let (id, blob) = c
        .app()
        .db
        .write(move |tx| {
            let mut message = message;
            if let Some(body) = body {
                message.update_body(tx, &body)?;
            }
            let attachment_given = attachment.is_some();
            let blob = attachment.flatten().map(|staged| save_staged(tx, staged)).transpose()?;
            if attachment_given {
                message.replace_attachment(tx, blob.as_ref().map(|blob| blob.id))?;
            }
            Ok((message.id, blob))
        })
        .await
        .map_err(db_error)?;
    if let Some(blob) = blob.filter(|blob| !blob.is_analyzed()) {
        let job_app = c.app().clone();
        c.app().jobs.perform_later("ActiveStorage::AnalyzeJob", async move {
            analyze_attachment(&job_app, blob).await.map(drop).map_err(|e| anyhow::anyhow!("{e:?}"))
        });
    }
    c.app().db.read(move |conn| Message::find(conn, id)).await.map_err(db_error)
}


/// `@message.destroy` then `@message.broadcast_remove`.
pub(crate) async fn destroy_message(c: &Ctx, room: &Room, message: &Message) -> Result<()> {
    destroy_and_broadcast(c.app(), room, message).await
}

/// [`destroy_message`] outside a request (`remove_banned_content` uses it too). A parent's
/// replies go with it, and their removal is broadcast; a reply's parent gets its new summary.
pub(crate) async fn destroy_and_broadcast(app: &App, room: &Room, message: &Message) -> Result<()> {
    let destroyed = message.clone();
    let replies = app
        .db
        .write(move |tx| {
            let replies = if destroyed.is_reply() { Vec::new() } else { Message::replies(tx.conn(), destroyed.id)? };
            destroyed.destroy(tx)?;
            Ok(replies)
        })
        .await
        .map_err(db_error)?;
    app.broadcasts.message_remove(room, message);
    for reply in &replies {
        app.broadcasts.message_remove(room, reply);
    }
    if let Some(parent_id) = message.thread_parent_id {
        broadcast_thread_summary(app, room, parent_id).await?;
    }
    Ok(())
}

// --- Broadcasts and webhooks -----------------------------------------------------------------------

/// `@message.broadcast_create`: the message partial appended to the room, then the unread pings.
pub(crate) async fn broadcast_create(c: &Ctx, room: &Room, message: &Message) -> Result<()> {
    let (app, room, message) = (c.app().clone(), room.clone(), message.clone());
    let base_url = page::renderer_base_url(c);
    c.app().db.read(move |conn| broadcast_create_on(conn, &app, &room, &message, &base_url)).await.map_err(db_error)
}

/// [`broadcast_create`] on a reader, with URLs at `base_url`; webhook replies use it too. A
/// reply goes to its thread instead (plans/threads.md).
pub(crate) fn broadcast_create_on(conn: &campfire_db::Connection, app: &App, room: &Room, message: &Message, base_url: &str) -> campfire_db::Result<()> {
    let presenter = Presenter::new(conn, app, None);
    let view = presenter.message(message)?;
    let account = campfire_db::Account::first(conn)?;
    let html = page::render_detached_at(app, account.as_ref(), base_url, |ctx| views::message(ctx, &view));
    let Some(parent_id) = message.thread_parent_id else {
        let partials = Rendered { message: Some(html), ..Rendered::default() };
        return app.broadcasts.message_create(conn, room, message, &partials);
    };
    let parent = Message::find(conn, parent_id)?;
    let audience = message.thread_audience_ids(conn, &*app.db.env().rich_text)?;
    let partials = Rendered { message: Some(html), thread_summary: Some(thread_summary_html(&presenter, &parent)?), ..Rendered::default() };
    app.broadcasts.reply_create(room, message, &parent, &audience, &partials);
    Ok(())
}

/// Replaces a thread's summary under its parent, after a reply went. Nothing to do when the
/// parent went too.
pub(crate) async fn broadcast_thread_summary(app: &App, room: &Room, parent_id: i64) -> Result<()> {
    let (app, room) = (app.clone(), room.clone());
    let read = app.clone();
    read.db
        .read(move |conn| {
            let Some(parent) = Message::find_by_id(conn, parent_id)? else { return Ok(()) };
            let presenter = Presenter::new(conn, &app, None);
            let partials = Rendered { thread_summary: Some(thread_summary_html(&presenter, &parent)?), ..Rendered::default() };
            app.broadcasts.thread_summary_replace(&room, &parent, &partials);
            Ok(())
        })
        .await
        .map_err(db_error)
}

fn thread_summary_html(presenter: &Presenter, parent: &Message) -> campfire_db::Result<String> {
    let view = presenter.message(parent)?;
    campfire_views::threads::SummaryPartial { message: &view }.render().map_err(|e| campfire_db::Error::Other(e.to_string()))
}

/// `broadcast_replace_to @room, :messages, target: [ @message, :presentation ], partial:
/// "messages/presentation", attributes: { maintain_scroll: true }`
pub(crate) async fn broadcast_replace(c: &Ctx, room: &Room, message: &Message) -> Result<()> {
    let (app, room, message) = (c.app().clone(), room.clone(), message.clone());
    let base_url = page::renderer_base_url(c);
    c.app().db.read(move |conn| broadcast_replace_on(conn, &app, &room, &message, &base_url)).await.map_err(db_error)
}

/// [`broadcast_replace`] on a reader, with URLs at `base_url`; a huddle's card ending uses it too.
pub(crate) fn broadcast_replace_on(conn: &campfire_db::Connection, app: &App, room: &Room, message: &Message, base_url: &str) -> campfire_db::Result<()> {
    let presenter = Presenter::new(conn, app, None);
    let view = presenter.message(message)?;
    let account = campfire_db::Account::first(conn)?;
    let html = page::render_detached_at(app, account.as_ref(), base_url, |ctx| views::PresentationPartial { ctx, message: &view }.render())
        .map_err(|e| campfire_db::Error::Other(e.to_string()))?;
    let partials = Rendered { message_presentation: Some(html), ..Rendered::default() };
    app.broadcasts.message_replace(room, message, &partials);
    Ok(())
}

/// `deliver_webhooks_to_bots`: every active bot in a direct room, else every mentioned active
/// bot, except the message's creator.
pub(crate) async fn deliver_webhooks_to_bots(c: &Ctx, room: &Room, message: &Message) -> Result<()> {
    deliver_webhooks_to_bots_on(c.app(), room, message).await
}

/// [`deliver_webhooks_to_bots`] outside a request (a scheduled message posting).
pub(crate) async fn deliver_webhooks_to_bots_on(app: &App, room: &Room, message: &Message) -> Result<()> {
    let (reader, room, eligible) = (app.clone(), room.clone(), message.clone());
    let bots: Vec<User> = app
        .db
        .read(move |conn| {
            let candidates =
                if room.direct() { room.active_bots(conn)? } else { eligible.mentionees(conn, &*reader.db.env().rich_text)? };
            Ok(candidates
                .into_iter()
                .filter(|user| user.role == Role::Bot && user.status == Status::Active && user.id != eligible.creator_id)
                .collect())
        })
        .await
        .map_err(db_error)?;
    if bots.is_empty() {
        return Ok(());
    }
    // bot.deliver_webhook_later(@message)
    let message_id = message.id;
    app.db
        .write(move |tx| bots.iter().try_for_each(|bot| bot.deliver_webhook_later(tx, message_id)))
        .await
        .map_err(db_error)
}

// --- Rendering ------------------------------------------------------------------------------------

/// Runs `f` with a presenter on a reader connection.
pub(crate) async fn present<T: Send + 'static>(
    c: &Ctx,
    f: impl FnOnce(&Presenter) -> campfire_db::Result<T> + Send + 'static,
) -> Result<T> {
    let app = c.app().clone();
    let request_host = Some(c.request.host());
    c.app()
        .db
        .read(move |conn| {
            let presenter = Presenter::new(conn, &app, request_host);
            // The Jbuilder partials (`json.cache!`) read the fragment cache on this thread.
            campfire_views::fragment_cache::with(&app.fragment_cache, || f(&presenter))
        })
        .await
        .map_err(db_error)
}

/// `render action: :room_not_found` (inside the layout).
async fn render_room_not_found(c: &mut Ctx) -> Result {
    c.respond_to(&[&format::HTML])?;
    page::content_in_application_layout(c, StatusCode::OK, |_| views::RoomNotFound.render()).await
}

#[cfg(test)]
mod tests;
