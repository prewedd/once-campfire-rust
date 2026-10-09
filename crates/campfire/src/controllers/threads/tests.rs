//! Request-level tests for threads (plans/threads.md), against the `default` parity seed.

use axum::http::{Method, StatusCode};
use campfire_db::{Message, Timeline};

use crate::controllers::presenters::test_support::*;

const TURBO_STREAM_ACCEPT: &str = "text/vnd.turbo-stream.html, text/html, application/xhtml+xml";

/// The room's newest top-level message: what the threads below hang off.
async fn last_message(app: &TestApp, room_id: i64) -> Message {
    app.db().read(move |conn| Message::last_page(conn, room_id)).await.unwrap().pop().unwrap()
}

async fn find(app: &TestApp, id: i64) -> Option<Message> {
    app.db().read(move |conn| Message::find_by_id(conn, id)).await.unwrap()
}

async fn by_client_id(app: &TestApp, client_message_id: &str) -> Message {
    let client_message_id = client_message_id.to_string();
    app.db()
        .read(move |conn| {
            let id: i64 =
                conn.query_row(r#"SELECT "id" FROM "messages" WHERE "client_message_id" = ?"#, [client_message_id], |r| r.get(0))?;
            Message::find(conn, id)
        })
        .await
        .unwrap()
}

async fn reply(browser: &mut Browser<'_>, room_id: i64, parent_id: i64, body: &str, client_message_id: &str) -> Reply {
    browser
        .write(
            Req::new(Method::POST, &format!("/rooms/{room_id}/messages?thread_parent_id={parent_id}"))
                .header("accept", TURBO_STREAM_ACCEPT)
                .form(&[("message[body]", body), ("message[client_message_id]", client_message_id)]),
        )
        .await
}

#[tokio::test]
async fn replying_in_a_thread() {
    let Some(app) = TestApp::boot().await else {
        return;
    };
    let parent = last_message(&app, ALL_TALK).await;
    let mut david = app.david();

    let response = reply(&mut david, ALL_TALK, parent.id, "<p>In the thread</p>", "thread-1").await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.text());
    assert!(
        response.text().starts_with(&format!(r#"<turbo-stream action="append" target="thread_messages_{}">"#, parent.id)),
        "{}",
        response.text()
    );
    assert!(response.text().contains(r#"id="message_thread-1""#));

    let reply = by_client_id(&app, "thread-1").await;
    assert_eq!(reply.thread_parent_id, Some(parent.id));
    assert!(find(&app, parent.id).await.unwrap().updated_at >= reply.created_at, "the parent is touched");
    let room = app.db().read(move |conn| Message::last_page(conn, ALL_TALK)).await.unwrap();
    assert_eq!(room.last().unwrap().id, parent.id, "the room's timeline is unchanged");

    let page = david.get(&format!("/rooms/{ALL_TALK}")).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.text().contains(&format!(r#"href="/rooms/{ALL_TALK}/threads/{}""#, parent.id)));
    assert!(page.text().contains(">1 reply<"), "the summary");
    assert!(!page.text().contains("In the thread"), "replies stay in their thread");
}

#[tokio::test]
async fn replies_to_a_reply_go_in_its_thread() {
    let Some(app) = TestApp::boot().await else {
        return;
    };
    let parent = last_message(&app, ALL_TALK).await;
    let mut david = app.david();
    reply(&mut david, ALL_TALK, parent.id, "first", "thread-a").await;
    let first = by_client_id(&app, "thread-a").await;

    let response = david
        .write(Req::new(Method::POST, &format!("/rooms/{ALL_TALK}/messages")).header("accept", TURBO_STREAM_ACCEPT).form(&[
            ("message[body]", "second"),
            ("message[client_message_id]", "thread-b"),
            ("message[thread_parent_id]", &first.id.to_string()),
        ]))
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.text());
    assert_eq!(by_client_id(&app, "thread-b").await.thread_parent_id, Some(parent.id));
}

#[tokio::test]
async fn a_thread_must_be_in_the_room() {
    let Some(app) = TestApp::boot().await else {
        return;
    };
    let mut david = app.david();
    david
        .write(
            Req::new(Method::POST, &format!("/rooms/{HQ}/messages"))
                .header("accept", TURBO_STREAM_ACCEPT)
                .form(&[("message[body]", "elsewhere"), ("message[client_message_id]", "thread-elsewhere")]),
        )
        .await;
    let elsewhere = by_client_id(&app, "thread-elsewhere").await;
    let response = reply(&mut david, ALL_TALK, elsewhere.id, "lost", "thread-x").await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);
    let missing = reply(&mut david, ALL_TALK, 0, "lost", "thread-y").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    // A blank thread is no thread: the room composer's message.
    let blank = david
        .write(
            Req::new(Method::POST, &format!("/rooms/{ALL_TALK}/messages?thread_parent_id="))
                .header("accept", TURBO_STREAM_ACCEPT)
                .form(&[("message[body]", "top"), ("message[client_message_id]", "thread-z")]),
        )
        .await;
    assert_eq!(blank.status, StatusCode::OK);
    assert_eq!(by_client_id(&app, "thread-z").await.thread_parent_id, None);
}

#[tokio::test]
async fn the_thread_view_and_its_links() {
    let Some(app) = TestApp::boot().await else {
        return;
    };
    let parent = last_message(&app, ALL_TALK).await;
    let mut david = app.david();
    reply(&mut david, ALL_TALK, parent.id, "<p>zebra reply</p>", "thread-v").await;
    let reply = by_client_id(&app, "thread-v").await;
    let thread = format!("/rooms/{ALL_TALK}/threads/{}", parent.id);

    let page = david.get(&thread).await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.text());
    let html = page.text();
    assert!(html.contains("<title>Thread in All Talk</title>"), "{html}");
    assert!(html.contains(&format!(r#"id="thread_messages_{}""#, parent.id)));
    assert!(html.contains(&format!(r#"data-message-id="{}""#, parent.id)) && html.contains("zebra reply"));
    assert!(html.contains(&format!(r#"action="/rooms/{ALL_TALK}/messages?thread_parent_id={}""#, parent.id)));
    assert!(html.contains(&format!(r#"href="{thread}/@{}""#, reply.id)), "the reply's permalink");

    let at_reply = david.get(&format!("{thread}/@{}", reply.id)).await;
    assert_eq!(at_reply.status, StatusCode::OK);
    assert!(at_reply.text().contains("zebra reply"));

    let old_link = david.get(&format!("/rooms/{ALL_TALK}/@{}", reply.id)).await;
    assert_eq!(old_link.status, StatusCode::FOUND);
    assert!(old_link.location().unwrap().ends_with(&format!("{thread}/@{}", reply.id)), "{:?}", old_link.location());

    let reply_as_thread = david.get(&format!("/rooms/{ALL_TALK}/threads/{}", reply.id)).await;
    assert_eq!(reply_as_thread.status, StatusCode::FOUND);
    assert!(reply_as_thread.location().unwrap().ends_with(&format!("{thread}/@{}", reply.id)));

    let top_level = david.get(&format!("/rooms/{ALL_TALK}/@{}", parent.id)).await;
    assert_eq!(top_level.status, StatusCode::OK, "a top-level message's permalink is still the room");

    assert_eq!(david.get(&format!("/rooms/{HQ}/threads/{}", parent.id)).await.status, StatusCode::NOT_FOUND, "another room");
    assert_ne!(david.get(&format!("/rooms/{DIRECT_KEVIN_BENDER}/threads/{}", parent.id)).await.status, StatusCode::OK);
    assert_eq!(app.anonymous().get(&thread).await.status, StatusCode::FOUND, "to sign in");
}

#[tokio::test]
async fn thread_pages_and_refreshes() {
    let Some(app) = TestApp::boot().await else {
        return;
    };
    let parent = last_message(&app, ALL_TALK).await;
    let mut david = app.david();
    reply(&mut david, ALL_TALK, parent.id, "one", "thread-p1").await;
    reply(&mut david, ALL_TALK, parent.id, "two", "thread-p2").await;
    let (first, second) = (by_client_id(&app, "thread-p1").await, by_client_id(&app, "thread-p2").await);
    let messages = format!("/rooms/{ALL_TALK}/threads/{}/messages", parent.id);

    let last = david.get(&messages).await;
    assert_eq!(last.status, StatusCode::OK);
    assert_eq!(last.text().matches(r#"data-controller="reply""#).count(), 3, "the parent and both replies");
    assert!(!last.text().contains("<html"));

    let before = david.get(&format!("{messages}?before={}", second.id)).await;
    assert_eq!(before.text().matches(r#"data-controller="reply""#).count(), 2);
    assert_eq!(david.get(&format!("{messages}?after={}", second.id)).await.status, StatusCode::NO_CONTENT);
    assert_eq!(david.get(&format!("{messages}?before={}", parent.id)).await.status, StatusCode::NO_CONTENT);

    let refresh = david
        .send(
            Req::new(
                Method::GET,
                &format!("/rooms/{ALL_TALK}/threads/{}/refresh?since={}", parent.id, first.created_at.jiff().as_millisecond() - 1),
            )
            .header("accept", TURBO_STREAM_ACCEPT),
        )
        .await;
    assert_eq!(refresh.status, StatusCode::OK, "{}", refresh.text());
    assert!(refresh.text().contains(&format!(r#"<turbo-stream action="append" target="thread_messages_{}">"#, parent.id)));
    assert!(refresh.text().contains("two"));

    // The room's own refresh never sends replies, only their parent's new summary.
    let room_refresh = david
        .send(
            Req::new(Method::GET, &format!("/rooms/{ALL_TALK}/refresh?since={}", first.created_at.jiff().as_millisecond() - 1))
                .header("accept", TURBO_STREAM_ACCEPT),
        )
        .await;
    assert!(!room_refresh.text().contains("thread-p2"), "{}", room_refresh.text());
    assert!(room_refresh.text().contains(&format!(r#"target="message_{}""#, parent.client_message_id)));
    assert!(room_refresh.text().contains("2 replies"));
}

#[tokio::test]
async fn uploads_in_a_thread() {
    let Some(app) = TestApp::boot().await else {
        return;
    };
    let parent = last_message(&app, ALL_TALK).await;
    let mut david = app.david();
    let png: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 3, 8, 2, 0, 0, 0, 59, 150, 57, 145, 0, 0, 0, 16,
        73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0, 71, 12, 56, 57, 0, 245, 49, 11, 245, 53, 123, 251, 130, 0, 0, 0, 0, 73, 69, 78, 68,
        174, 66, 96, 130,
    ];
    // What the thread composer's FileUploader posts: the file and client id, to the form's action.
    let response = david
        .write(
            Req::new(Method::POST, &format!("/rooms/{ALL_TALK}/messages?thread_parent_id={}", parent.id))
                .header("accept", "*/*")
                .multipart(&[("message[client_message_id]", "thread-upload")], ("message[attachment]", "red.png", "image/png", png)),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.text());
    assert!(response.text().contains(&format!(r#"target="thread_messages_{}""#, parent.id)));
    assert_eq!(by_client_id(&app, "thread-upload").await.thread_parent_id, Some(parent.id));
}

#[tokio::test]
async fn deleting_replies_and_threads() {
    let Some(app) = TestApp::boot().await else {
        return;
    };
    let mut david = app.david();
    // A top-level message of David's own, so he can delete it.
    david
        .write(
            Req::new(Method::POST, &format!("/rooms/{ALL_TALK}/messages"))
                .header("accept", TURBO_STREAM_ACCEPT)
                .form(&[("message[body]", "parent"), ("message[client_message_id]", "thread-parent")]),
        )
        .await;
    let parent = by_client_id(&app, "thread-parent").await;
    reply(&mut david, ALL_TALK, parent.id, "one", "thread-d1").await;
    reply(&mut david, ALL_TALK, parent.id, "two", "thread-d2").await;
    let (first, second) = (by_client_id(&app, "thread-d1").await, by_client_id(&app, "thread-d2").await);

    let gone = david
        .write(Req::new(Method::DELETE, &format!("/rooms/{ALL_TALK}/messages/{}", first.id)).header("accept", TURBO_STREAM_ACCEPT))
        .await;
    assert_eq!(gone.status, StatusCode::OK, "{}", gone.text());
    assert!(gone.text().contains(r#"<turbo-stream action="remove" target="message_thread-d1">"#));
    assert!(find(&app, first.id).await.is_none());
    let parent_id = parent.id;
    let summary = app.db().read(move |conn| Message::thread_summary(conn, parent_id)).await.unwrap().unwrap();
    assert_eq!(summary.reply_count, 1);

    david.write(Req::new(Method::DELETE, &format!("/rooms/{ALL_TALK}/messages/{}", parent.id)).header("accept", TURBO_STREAM_ACCEPT)).await;
    assert!(find(&app, parent.id).await.is_none());
    assert!(find(&app, second.id).await.is_none(), "the thread goes with its parent");
    assert_eq!(david.get(&format!("/rooms/{ALL_TALK}/threads/{}", parent.id)).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bots_can_post_into_a_thread() {
    let Some(app) = TestApp::boot().await else {
        return;
    };
    let parent = last_message(&app, ALL_TALK).await;
    let mut bot = app.anonymous();
    let response = bot
        .write(
            Req::new(Method::POST, &format!("/rooms/{ALL_TALK}/{BENDER_KEY}/messages?thread_parent_id={}", parent.id))
                .body("Beep in a thread"),
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.text());
    let parent_id = parent.id;
    let thread = app.db().read(move |conn| Timeline::Thread(parent_id).last_page(conn)).await.unwrap();
    let posted = thread.last().unwrap();
    assert_eq!((posted.creator_id, posted.thread_parent_id), (BENDER, Some(parent.id)));
}
