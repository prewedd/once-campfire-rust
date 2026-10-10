//! Threads (plans/threads.md): replies, their thread, and who hears about them.

use super::*;
use crate::models::push_subscription::PushSubscription;
use crate::rich_text::mention_attachment_for;
use crate::{Membership, Message, NewMessage, Room, Timeline, Webhook};

/// Posts a message a second after the last, so creation order is unambiguous.
fn post(
    t: &TestDb,
    room: &str,
    creator: &str,
    body: String,
    thread_parent_id: Option<i64>,
) -> Message {
    t.travel(1);
    let attributes = NewMessage {
        room_id: id(room),
        creator_id: id(creator),
        client_message_id: None,
        body: Some(body),
        attachment_blob_id: None,
        thread_parent_id,
    };
    t.write(move |tx| Message::create(tx, attributes))
}

fn reload(t: &TestDb, message: &Message) -> Option<Message> {
    let id = message.id;
    t.read(|c| Message::find_by_id(c, id))
}

fn ids(messages: &[Message]) -> Vec<i64> {
    messages.iter().map(|m| m.id).collect()
}

fn unread(t: &TestDb, membership: &str) -> bool {
    let id = id(membership);
    t.read(|c| Membership::find(c, id)).unread()
}

fn mark_read(t: &TestDb, memberships: &[&str]) {
    let ids: Vec<i64> = memberships.iter().map(|label| id(label)).collect();
    t.write(move |tx| {
        for id in ids {
            Membership::find(tx.conn(), id)?.read(tx)?;
        }
        Ok(())
    });
}

#[test]
fn replies_stay_out_of_the_room_timeline() {
    let t = TestDb::with_mentions();
    let parent = post(
        &t,
        "designers",
        "kevin",
        "Thoughts on the logo?".into(),
        None,
    );
    let reply = post(&t, "designers", "david", "Bigger".into(), Some(parent.id));
    let next = post(&t, "designers", "jason", "Lunch?".into(), None);
    let designers = id("designers");

    let room = ids(&t.read(|c| Message::last_page(c, designers)));
    assert!(room.ends_with(&[parent.id, next.id]), "{room:?}");
    assert!(!room.contains(&reply.id));
    assert_eq!(
        ids(&t.read(|c| Message::page_after(c, designers, &parent))),
        vec![next.id]
    );
    assert!(
        !ids(&t.read(|c| Message::page_created_since(c, designers, parent.created_at)))
            .contains(&reply.id)
    );
    assert!(!t.read(|c| Message::exists_after(c, designers, &next)));

    let all = t.read(|c| Message::for_room(c, designers)).len() as i64;
    assert_eq!(t.read(|c| Message::count_in_room(c, designers)), all - 1);
}

#[test]
fn a_thread_is_its_parent_then_its_replies() {
    let t = TestDb::with_mentions();
    let parent = post(
        &t,
        "designers",
        "kevin",
        "Thoughts on the logo?".into(),
        None,
    );
    let first = post(&t, "designers", "david", "Bigger".into(), Some(parent.id));
    post(&t, "designers", "jason", "Unrelated".into(), None);
    let second = post(&t, "designers", "jz", "Bolder".into(), Some(parent.id));
    let thread = Timeline::Thread(parent.id);

    assert_eq!(
        ids(&t.read(|c| thread.last_page(c))),
        vec![parent.id, first.id, second.id]
    );
    assert_eq!(
        ids(&t.read(|c| thread.page_before(c, &second))),
        vec![parent.id, first.id]
    );
    assert_eq!(
        ids(&t.read(|c| thread.page_after(c, &parent))),
        vec![first.id, second.id]
    );
    assert_eq!(
        ids(&t.read(|c| thread.page_around(c, &first))),
        vec![parent.id, first.id, second.id]
    );
    assert_eq!(
        ids(&t.read(|c| thread.page_created_since(c, first.created_at))),
        vec![second.id]
    );
    assert!(!t.read(|c| thread.paged(c)));
    assert_eq!(t.read(|c| Message::replies(c, parent.id)).len(), 2);
}

#[test]
fn a_reply_touches_its_parent_and_shows_in_its_summary() {
    let t = TestDb::with_mentions();
    let parent = post(
        &t,
        "designers",
        "kevin",
        "Thoughts on the logo?".into(),
        None,
    );
    assert_eq!(t.read(|c| Message::thread_summary(c, parent.id)), None);

    let reply = post(&t, "designers", "david", "Bigger".into(), Some(parent.id));
    assert!(reload(&t, &parent).unwrap().updated_at >= reply.created_at);

    let summary = t.read(|c| Message::thread_summary(c, parent.id)).unwrap();
    assert_eq!(summary.reply_count, 1);
    assert_eq!(summary.last_reply_at, reply.created_at);
    assert_eq!(summary.participant_ids, vec![id("david")]);
}

#[test]
fn the_summary_shows_the_latest_three_repliers() {
    let t = TestDb::with_mentions();
    let parent = post(
        &t,
        "designers",
        "kevin",
        "Thoughts on the logo?".into(),
        None,
    );
    for replier in ["jason", "jz", "david", "jason", "kevin"] {
        post(&t, "designers", replier, "Hm".into(), Some(parent.id));
    }
    let summary = t.read(|c| Message::thread_summary(c, parent.id)).unwrap();
    assert_eq!(summary.reply_count, 5);
    assert_eq!(
        summary.participant_ids,
        vec![id("kevin"), id("jason"), id("david")]
    );
}

#[test]
fn replies_unread_the_room_only_for_the_thread_audience() {
    let t = TestDb::with_mentions();
    let parent = post(
        &t,
        "designers",
        "kevin",
        "Thoughts on the logo?".into(),
        None,
    );
    mark_read(
        &t,
        &[
            "david_designers",
            "jason_designers",
            "jz_designers",
            "kevin_designers",
        ],
    );

    post(
        &t,
        "designers",
        "david",
        format!("Ask {}", mention_attachment_for(id("jz"))),
        Some(parent.id),
    );

    assert!(unread(&t, "kevin_designers"), "the parent's author");
    assert!(unread(&t, "jz_designers"), "mentioned");
    assert!(
        !unread(&t, "jason_designers"),
        "not in the thread, though involved in everything"
    );
    assert!(!unread(&t, "david_designers"), "the reply's author");
}

#[test]
fn replies_are_pushed_to_the_thread_audience_unless_muted() {
    let t = TestDb::with_mentions();
    let parent = post(
        &t,
        "designers",
        "kevin",
        "Thoughts on the logo?".into(),
        None,
    );
    post(&t, "designers", "jason", "Bigger".into(), Some(parent.id));
    let reply = post(
        &t,
        "designers",
        "david",
        format!("Ask {}", mention_attachment_for(id("jz"))),
        Some(parent.id),
    );
    let now = t.now();

    let pushed_to = |t: &TestDb| {
        let (payload, subscriptions, rest) =
            t.read(|c| PushSubscription::pushes_for(c, &UnverifiedMentions, &reply, now));
        assert!(rest.is_empty());
        let mut users: Vec<i64> = subscriptions.iter().map(|s| s.user_id).collect();
        users.sort_unstable();
        (payload, users)
    };
    let (payload, users) = pushed_to(&t);
    let mut expected = vec![id("kevin"), id("jason"), id("jz")];
    expected.sort_unstable();
    assert_eq!(users, expected);
    assert_eq!(payload.title, "Thread in Designers");
    assert_eq!(payload.body, "David: Ask @JZ");
    assert_eq!(
        payload.path,
        format!(
            "/rooms/{}/threads/{}/@{}",
            id("designers"),
            parent.id,
            reply.id
        )
    );

    t.write(|tx| {
        let mut membership = Membership::find(tx.conn(), id("kevin_designers"))?;
        membership.update_involvement(tx, crate::Involvement::Nothing)
    });
    let (_, users) = pushed_to(&t);
    assert!(!users.contains(&id("kevin")), "muted the room");
}

#[test]
fn a_direct_reply_push_names_its_sender() {
    let t = TestDb::with_mentions();
    let parent = post(&t, "david_and_jason", "jason", "Coffee?".into(), None);
    let reply = post(
        &t,
        "david_and_jason",
        "david",
        "Sure".into(),
        Some(parent.id),
    );
    let now = t.now();
    let (payload, subscriptions, _) =
        t.read(|c| PushSubscription::pushes_for(c, &UnverifiedMentions, &reply, now));
    assert_eq!(payload.title, "David in a thread");
    assert_eq!(payload.body, "Sure");
    assert!(subscriptions.iter().all(|s| s.user_id == id("jason")) && !subscriptions.is_empty());
}

#[test]
fn destroying_a_parent_destroys_its_thread() {
    let t = TestDb::with_mentions();
    let parent = post(
        &t,
        "designers",
        "kevin",
        "Thoughts on the logo?".into(),
        None,
    );
    let first = post(
        &t,
        "designers",
        "david",
        "zebrastripes".into(),
        Some(parent.id),
    );
    let second = post(&t, "designers", "jz", "Bolder".into(), Some(parent.id));

    let destroyed = parent.clone();
    t.write(move |tx| destroyed.destroy(tx));

    assert!(reload(&t, &parent).is_none());
    assert!(reload(&t, &first).is_none());
    assert!(reload(&t, &second).is_none());
    let designers = id("designers");
    assert!(
        t.read(|c| Message::search_in_room(c, designers, "zebrastripes"))
            .is_empty()
    );
}

#[test]
fn destroying_a_reply_touches_its_parent() {
    let t = TestDb::with_mentions();
    let parent = post(
        &t,
        "designers",
        "kevin",
        "Thoughts on the logo?".into(),
        None,
    );
    let reply = post(&t, "designers", "david", "Bigger".into(), Some(parent.id));
    t.travel(1);
    let destroyed = reply.clone();
    t.write(move |tx| destroyed.destroy(tx));

    assert!(reload(&t, &reply).is_none());
    assert!(reload(&t, &parent).unwrap().updated_at > reply.created_at);
    assert_eq!(t.read(|c| Message::thread_summary(c, parent.id)), None);
}

#[test]
fn destroying_a_room_with_threads() {
    let t = TestDb::with_mentions();
    let parent = post(
        &t,
        "designers",
        "kevin",
        "Thoughts on the logo?".into(),
        None,
    );
    post(&t, "designers", "david", "Bigger".into(), Some(parent.id));
    let designers = id("designers");
    t.write(move |tx| Room::find(tx.conn(), designers)?.destroy(tx));
    assert!(t.read(|c| Message::for_room(c, designers)).is_empty());
}

#[test]
fn search_finds_replies() {
    let t = TestDb::with_mentions();
    let parent = post(
        &t,
        "designers",
        "kevin",
        "Thoughts on the logo?".into(),
        None,
    );
    let reply = post(
        &t,
        "designers",
        "david",
        "zebrastripes".into(),
        Some(parent.id),
    );
    let designers = id("designers");
    assert_eq!(
        ids(&t.read(|c| Message::search_in_room(c, designers, "zebrastripes"))),
        vec![reply.id]
    );
}

#[test]
fn webhook_payloads_name_a_replys_thread() {
    let t = TestDb::with_mentions();
    let parent = post(&t, "watercooler", "david", "Hey bot".into(), None);
    let reply = post(
        &t,
        "watercooler",
        "david",
        "Still there?".into(),
        Some(parent.id),
    );
    let payload = |message: &Message| {
        t.read(|c| {
            let webhook = Webhook::find_by_user(c, id("bender"))?.unwrap();
            webhook.payload(c, &UnverifiedMentions, message, "/bot", "/at")
        })
    };
    assert!(!payload(&parent).contains("thread_parent_id"));
    assert!(
        payload(&reply).ends_with(&format!(
            r#""path":"/at","thread_parent_id":{}}}}}"#,
            parent.id
        )),
        "{}",
        payload(&reply)
    );
}

#[test]
fn the_audience_of_a_top_level_message_is_empty() {
    let t = TestDb::with_mentions();
    let parent = post(
        &t,
        "designers",
        "kevin",
        format!("Hi {}", mention_attachment_for(id("jz"))),
        None,
    );
    assert!(
        t.read(|c| parent.thread_audience_ids(c, &UnverifiedMentions))
            .is_empty()
    );
}
