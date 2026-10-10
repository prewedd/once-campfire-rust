//! Saved for later (plans/redesign.md): only reachable messages, newest saved first.

use super::*;
use crate::{Message, Room, SavedMessage};

fn a_message_in(t: &TestDb, room: &str) -> i64 {
    let room_id = id(room);
    t.read(move |c| Message::last_page(c, room_id)).last().expect("a message").id
}

/// Two messages in rooms David is in.
fn two_of_davids_messages(t: &TestDb) -> (i64, i64) {
    let david = id("david");
    let ids: Vec<i64> = t.read(move |c| {
        let mut statement = c.prepare(r#"SELECT "messages"."id" FROM "messages" INNER JOIN "memberships" ON "memberships"."room_id" = "messages"."room_id" WHERE "memberships"."user_id" = ? ORDER BY "messages"."id" LIMIT 2"#)?;
        let ids = statement.query_map([david], |row| row.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
        Ok(ids)
    });
    (ids[0], ids[1])
}

#[test]
fn saved_messages_come_back_newest_first_and_can_be_unsaved() {
    let t = TestDb::new();
    let david = id("david");
    let (first, second) = two_of_davids_messages(&t);
    assert!(t.write(move |tx| SavedMessage::save(tx, david, first, 1_000)));
    assert!(t.write(move |tx| SavedMessage::save(tx, david, second, 2_000)));
    assert!(t.write(move |tx| SavedMessage::save(tx, david, first, 3_000)), "saving twice keeps the first time");
    assert_eq!(t.read(move |c| SavedMessage::ids(c, david)), vec![second, first]);
    assert_eq!(t.read(move |c| SavedMessage::messages(c, david)).iter().map(|m| m.id).collect::<Vec<_>>(), vec![second, first]);
    t.write(move |tx| SavedMessage::unsave(tx, david, second));
    assert_eq!(t.read(move |c| SavedMessage::ids(c, david)), vec![first]);
}

#[test]
fn only_messages_you_can_reach_are_saved() {
    let t = TestDb::new();
    let (kevin, david) = (id("kevin"), id("david"));
    let watercooler = id("watercooler");
    let reachable = t.read(move |c| Ok(Room::find(c, watercooler)?.user_ids(c)?.contains(&kevin)));
    let message = a_message_in(&t, "watercooler");
    assert_eq!(t.write(move |tx| SavedMessage::save(tx, kevin, message, 1_000)), reachable);
    assert!(!t.write(move |tx| SavedMessage::save(tx, david, i64::MAX, 1_000)), "no such message");
}
