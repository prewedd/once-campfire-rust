//! Pinned messages (plans/redesign.md): members pin and unpin; a room lists its own.

use super::*;
use crate::{Message, PinnedMessage, Room};

#[test]
fn members_pin_and_unpin_a_rooms_messages() {
    let t = TestDb::new();
    let (david, room_id) = (id("david"), id("watercooler"));
    let messages = t.read(move |c| Message::last_page(c, room_id));
    let (older, newer) = (messages[0].id, messages[messages.len() - 1].id);
    assert_eq!(t.write(move |tx| PinnedMessage::pin(tx, david, older, 1_000)), Some(room_id));
    assert_eq!(t.write(move |tx| PinnedMessage::pin(tx, david, newer, 2_000)), Some(room_id));
    assert_eq!(t.read(move |c| PinnedMessage::ids(c, room_id)), vec![newer, older], "most recently pinned first");
    assert_eq!(t.write(move |tx| PinnedMessage::unpin(tx, david, newer)), Some(room_id));
    assert_eq!(t.read(move |c| PinnedMessage::ids(c, room_id)), vec![older]);
}

#[test]
fn only_members_pin() {
    let t = TestDb::new();
    let room_id = id("watercooler");
    let members = t.read(move |c| Room::find(c, room_id)?.user_ids(c));
    let outsider: i64 = t.read(move |c| {
        let ids: Vec<i64> = c.prepare(r#"SELECT "id" FROM "users""#)?.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(ids.into_iter().find(|id| !members.contains(id)).unwrap_or(i64::MAX))
    });
    let message = t.read(move |c| Message::last_page(c, room_id))[0].id;
    assert_eq!(t.write(move |tx| PinnedMessage::pin(tx, outsider, message, 1_000)), None);
    assert!(t.read(move |c| PinnedMessage::ids(c, room_id)).is_empty());
}
