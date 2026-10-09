//! Threads (plans/threads.md): this app's own markup, which the parity goldens leave out.

mod messages_support;

use askama::Template;
use campfire_views::messages::{self, MessageItem, MessageView, ThreadView, UserView};
use campfire_views::rooms::{self, ShowView};
use campfire_views::threads;
use messages_support::golden;

fn message() -> (messages_support::Golden, MessageView) {
    let g = golden("messages_show_text");
    let message: MessageView = g.input();
    (g, message)
}

fn render(g: &messages_support::Golden, message: &MessageView) -> String {
    g.render(|ctx| messages::MessagePartial { ctx, message }.render().unwrap())
}

fn user(id: i64, name: &str) -> UserView {
    UserView { id, name: name.into(), title: name.into(), avatar_url: format!("/users/{id}/avatar") }
}

#[test]
fn a_top_level_message_can_be_replied_to_in_a_thread() {
    let (g, message) = message();
    let html = render(&g, &message);
    let thread = format!("/rooms/{}/threads/{}", message.room_id, message.id);
    assert!(html.contains(&format!(r#"title="Reply in thread" aria-label="Reply in thread" href="{thread}""#)), "{html}");
    assert!(html.contains(&format!(r#"message__thread-btn--quick" data-turbo-frame="_top" title="Reply in thread" href="{thread}""#)), "one click from the hover toolbar");
    // No replies yet: the summary is there to be replaced, but empty.
    assert!(
        html.contains(&format!(r#"<div id="thread_summary_message_{}" class="message__thread"></div>"#, message.client_message_id)),
        "{html}"
    );
    assert!(!html.contains("data-thread-parent-id"));
}

#[test]
fn a_message_with_replies_shows_its_thread_summary() {
    let (g, mut message) = message();
    message.thread = Some(ThreadView {
        reply_count: 3,
        last_reply_at: "2026-10-09T10:00:00Z".parse().unwrap(),
        participants: vec![user(1, "Jason"), user(2, "JZ")],
    });
    let html = render(&g, &message);
    let summary = threads::SummaryPartial { message: &message }.render().unwrap();
    assert!(html.contains(&summary), "the partial broadcasts replace the summary with");
    assert!(summary.contains(&format!(r#"href="/rooms/{}/threads/{}""#, message.room_id, message.id)));
    assert!(summary.contains("3 replies"));
    assert!(summary.contains(r#"<time datetime="2026-10-09T10:00:00Z" data-local-time-target="datetime">"#));
    assert!(summary.contains(r#"src="/users/1/avatar""#) && summary.contains(r#"title="JZ""#));

    message.thread.as_mut().unwrap().reply_count = 1;
    assert!(threads::SummaryPartial { message: &message }.render().unwrap().contains("1 reply<"));
}

#[test]
fn a_reply_links_to_its_place_in_the_thread() {
    let (g, mut message) = message();
    message.thread_parent_id = Some(42);
    let html = render(&g, &message);
    let at = format!("/rooms/{}/threads/42/@{}", message.room_id, message.id);
    assert!(html.contains(&format!(r#"class="message__permalink" href="{at}""#)), "{html}");
    assert!(html.contains(&format!(r#"data-copy-to-clipboard-url-value="{at}""#)));
    assert!(html.contains(r#"data-thread-parent-id="42""#));
    assert!(!html.contains("Reply in thread"), "threads are one level deep");
}

fn thread_show() -> (messages_support::Golden, threads::ShowView) {
    let g = golden("rooms_show_member");
    let room: ShowView = g.input();
    let show = threads::ShowView {
        room: room.room.clone(),
        parent_id: 42,
        updated_at: room.updated_at,
        user: room.user.clone(),
        messages: room.messages.clone(),
        messages_stream_name: room.messages_stream_name.clone(),
    };
    (g, show)
}

#[test]
fn the_thread_view_posts_into_the_thread() {
    let (g, show) = thread_show();
    let html = g.render(|ctx| threads::Show { ctx, show: &show }.render().unwrap());
    let room_id = show.room.id;
    assert!(html.contains(r#"<div id="thread_messages_42" class="messages messages--thread""#), "{html}");
    assert!(html.contains(&format!(r#"data-messages-page-url-value="http://campfire.test/rooms/{room_id}/threads/42/messages""#)));
    assert!(html.contains(&format!(r#"data-refresh-room-url-value="http://campfire.test/rooms/{room_id}/threads/42/refresh""#)));
    assert!(html.contains(&format!(r#"data-composer-thread-id-value="42" action="/rooms/{room_id}/messages?thread_parent_id=42""#)));
    assert!(html.contains(r#"data-typing-notifications-thread-id-value="42""#));
    assert!(html.contains(r#"aria-label="Reply in thread""#));
    assert!(html.contains(&format!(r#"href="/rooms/{room_id}/@42""#)), "back to the room, at the parent");
    assert!(html.contains(&format!(r#"signed-stream-name="{}""#, show.messages_stream_name)));
    assert!(html.contains(&format!(r#"<meta name="current-room-id" content="{room_id}">"#)));
}

#[test]
fn the_room_composer_posts_into_the_room() {
    let g = golden("rooms_show_member");
    let show: ShowView = g.input();
    let html = g.render(|ctx| rooms::Show { ctx, show: &show }.render().unwrap());
    assert!(html.contains(&format!(r#"data-composer-room-id-value="{}" action="/rooms/{}/messages""#, show.room.id, show.room.id)));
    assert!(!html.contains("thread-id-value"));
    assert!(html.contains(r#"aria-label="Write a message""#));
}

#[test]
fn reply_streams_target_the_thread() {
    let (g, show) = thread_show();
    let item: &MessageItem = &show.messages[0];
    let create = g.render(|ctx| threads::CreateStream { ctx, message: item, parent_id: 42 }.render().unwrap());
    assert!(create.starts_with(r#"<turbo-stream action="append" target="thread_messages_42">"#), "{create}");

    let refresh = threads::RefreshView { parent_id: 42, new_messages: vec![item.clone()], updated_messages: vec![item.clone()] };
    let html = g.render(|ctx| threads::RefreshShow { ctx, refresh: &refresh }.render().unwrap());
    assert!(html.contains(r#"<turbo-stream action="append" target="thread_messages_42">"#));
    assert!(html.contains(&format!(r#"<turbo-stream action="replace" target="{}">"#, item.dom_id(""))));
}
