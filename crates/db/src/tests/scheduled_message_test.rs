//! Scheduled messages (plans/scheduled-messages.md): posted once, when due, by members only.

use super::*;
use crate::{Membership, Message, NewScheduledMessage, ScheduledMessage};

fn schedule(t: &TestDb, room: &str, creator: &str, send_at: i64) -> ScheduledMessage {
    let new = NewScheduledMessage { room_id: id(room), creator_id: id(creator), thread_parent_id: None, body: "<div>Later</div>".into(), send_at };
    t.write(move |tx| ScheduledMessage::create(tx, new, 1_000))
}

#[test]
fn a_scheduled_message_is_posted_once_when_due() {
    let t = TestDb::new();
    let scheduled = schedule(&t, "pets", "david", 2_000);
    assert!(t.read(|c| ScheduledMessage::due(c, 1_999, 10)).is_empty(), "not before its time");
    assert_eq!(t.read(|c| ScheduledMessage::due(c, 2_000, 10)), vec![scheduled.id]);

    let id = scheduled.id;
    let message = t.write(move |tx| ScheduledMessage::post(tx, id)).expect("posted");
    assert_eq!((message.room_id, message.creator_id), (id_of("pets"), id_of("david")));
    assert_eq!(t.read(|c| ScheduledMessage::find(c, id)).message_id, Some(message.id));
    assert!(t.read(|c| ScheduledMessage::due(c, 9_999, 10)).is_empty(), "no longer due");
    assert_eq!(t.write(move |tx| ScheduledMessage::post(tx, id)), None, "never twice");
    let posted = message.id;
    assert!(t.read(move |c| Message::find_by_id(c, posted)).is_some());
}

#[test]
fn someone_who_left_the_room_has_their_scheduled_messages_dropped() {
    let t = TestDb::new();
    let scheduled = schedule(&t, "pets", "jason", 2_000);
    let membership = id("jason_pets");
    t.write(move |tx| Membership::find(tx.conn(), membership)?.destroy(tx));
    let id = scheduled.id;
    assert_eq!(t.write(move |tx| ScheduledMessage::post(tx, id)), None);
    assert!(t.read(move |c| ScheduledMessage::pending_for(c, scheduled.creator_id, None)).is_empty());
}

#[test]
fn deleted_and_rescheduled_ones_wait_or_go() {
    let t = TestDb::new();
    let mut later = schedule(&t, "pets", "david", 2_000);
    let gone = schedule(&t, "pets", "david", 2_000);
    t.write(move |tx| {
        later.reschedule(tx, 5_000, None)?;
        gone.destroy(tx)
    });
    assert!(t.read(|c| ScheduledMessage::due(c, 4_999, 10)).is_empty());
    assert_eq!(t.read(|c| ScheduledMessage::pending_for(c, id_of("david"), Some(id_of("pets")))).len(), 1);
}

fn id_of(label: &str) -> i64 {
    id(label)
}
