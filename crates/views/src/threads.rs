//! Threads (plans/threads.md): the thread view, its pages and refreshes, and the turbo streams
//! that put a reply in its thread.

use askama::Template;
use jiff::Timestamp;

use crate::ViewContext;
use crate::helpers as h;
use crate::layouts::Page;
use crate::messages::support::epoch_ms;
use crate::messages::{MessageItem, UserView};
use crate::rooms::RoomView;

/// No thread: the room composer's `thread_id`.
pub const NONE: Option<i64> = None;

/// The id of a thread's message list: where its replies are appended. Only the thread view has
/// it, so the room view ignores what's broadcast there.
pub fn messages_dom_id(parent_id: i64) -> String {
    format!("thread_messages_{parent_id}")
}

/// What the thread view shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ShowView {
    pub room: RoomView,
    /// The thread's top-level message.
    pub parent_id: i64,
    /// The parent's `updated_at` (every reply touches it), the refresh controller's `loaded_at`.
    pub updated_at: Timestamp,
    /// `Current.user`, for the client-side message template.
    pub user: UserView,
    /// The parent then its replies, a page at a time like a room.
    pub messages: Vec<MessageItem>,
    /// The room's message stream, which carries the thread's broadcasts too.
    pub messages_stream_name: String,
}

impl ShowView {
    pub fn messages_dom_id(&self) -> String {
        messages_dom_id(self.parent_id)
    }

    pub fn room_at_parent_path(&self) -> String {
        campfire_routes::room_at_message(self.room.id, self.parent_id)
    }
}

/// `threads/show`.
#[derive(Template)]
#[template(path = "threads/show.html", blocks = ["head", "content"])]
pub struct Show<'a> {
    pub ctx: &'a ViewContext<'a>,
    pub show: &'a ShowView,
}

impl Page for Show<'_> {
    fn page_title(&self) -> Option<String> {
        Some(format!("Thread in {}", self.show.room.display_name))
    }

    fn body_class(&self) -> Option<&str> {
        Some("sidebar thread")
    }
}

impl Show<'_> {
    fn loaded_at(&self) -> i64 {
        epoch_ms(self.show.updated_at)
    }
}

/// The turbo frame the room page shows threads and profiles in (plans/redesign.md).
pub const PANE_FRAME: &str = "flexpane";

/// `threads/pane`: the thread in the room page's right-hand panel.
#[derive(Template)]
#[template(path = "threads/pane.html")]
pub struct Pane<'a> {
    pub ctx: &'a ViewContext<'a>,
    pub show: &'a ShowView,
}

impl Pane<'_> {
    fn loaded_at(&self) -> i64 {
        epoch_ms(self.show.updated_at)
    }
}

/// What changed in a thread since the client last loaded it.
#[derive(Clone, Debug, PartialEq)]
pub struct RefreshView {
    pub parent_id: i64,
    pub new_messages: Vec<MessageItem>,
    pub updated_messages: Vec<MessageItem>,
}

/// `threads/refreshes/show.turbo_stream`, like `rooms/refreshes/show.turbo_stream`.
#[derive(Template)]
#[template(path = "threads/refresh.turbo_stream.html")]
pub struct RefreshShow<'a> {
    pub ctx: &'a ViewContext<'a>,
    pub refresh: &'a RefreshView,
}

/// A reply, appended to its thread: the poster's response to creating it.
#[derive(Template)]
#[template(path = "threads/create.turbo_stream.html")]
pub struct CreateStream<'a> {
    pub ctx: &'a ViewContext<'a>,
    pub message: &'a MessageItem,
    pub parent_id: i64,
}

/// `messages/_thread_summary` on its own, to replace a parent's summary in place.
#[derive(Template)]
#[template(path = "messages/_thread_summary.html")]
pub struct SummaryPartial<'a> {
    pub message: &'a crate::messages::MessageView,
}
