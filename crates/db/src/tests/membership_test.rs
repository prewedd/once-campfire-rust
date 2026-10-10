//! `test/models/membership_test.rb`

use super::*;
use crate::Membership;

const TTL_PLUS_ONE: i64 = 61;

fn membership(t: &TestDb) -> Membership {
    t.read(|c| Membership::find(c, id("david_watercooler")))
}

/// Runs a Connectable method and returns the updated in-memory membership.
fn run(
    t: &TestDb,
    membership: Membership,
    f: fn(&mut Membership, &mut Tx<'_>) -> Result<()>,
) -> Membership {
    t.write(move |tx| {
        let mut m = membership;
        f(&mut m, tx)?;
        Ok(m)
    })
}

fn connected(t: &TestDb, m: Membership) -> Membership {
    run(t, m, |m, tx| m.connected(tx))
}

fn disconnected(t: &TestDb, m: Membership) -> Membership {
    run(t, m, |m, tx| m.disconnected(tx))
}

fn connected_exists(t: &TestDb, m: &Membership) -> bool {
    let now = t.now();
    t.read(|c| Membership::connected_exists(c, m.id, now))
}

fn disconnected_exists(t: &TestDb, m: &Membership) -> bool {
    let now = t.now();
    t.read(|c| Membership::disconnected_exists(c, m.id, now))
}

#[test]
fn connected_scope() {
    let t = TestDb::new();
    let m = connected(&t, membership(&t));
    assert!(connected_exists(&t, &m));

    let m = disconnected(&t, m);
    assert!(!connected_exists(&t, &m));

    t.travel(TTL_PLUS_ONE);
    assert!(!connected_exists(&t, &m));
}

#[test]
fn disconnected_scope() {
    let t = TestDb::new();
    let m = disconnected(&t, membership(&t));
    assert!(disconnected_exists(&t, &m));

    let m = connected(&t, m);
    assert!(!disconnected_exists(&t, &m));

    t.travel(TTL_PLUS_ONE);
    assert!(disconnected_exists(&t, &m));
}

#[test]
fn connected_is_false_when_connection_is_stale() {
    let t = TestDb::new();
    let m = connected(&t, membership(&t));
    t.travel(TTL_PLUS_ONE);
    assert!(!m.is_connected(t.now()));
}

#[test]
fn connecting() {
    let t = TestDb::new();
    let m = connected(&t, membership(&t));
    assert!(m.is_connected(t.now()));
    assert_eq!(m.connections, 1);

    let m = connected(&t, m);
    assert_eq!(m.connections, 2);
    assert_eq!(t.read(|c| Membership::find(c, m.id)).connections, 2);
}

#[test]
fn connecting_resets_stale_connection_count() {
    let t = TestDb::new();
    let m = connected(&t, connected(&t, membership(&t)));
    assert_eq!(m.connections, 2);

    t.travel(TTL_PLUS_ONE);
    let m = connected(&t, m);
    assert_eq!(m.connections, 1);
}

#[test]
fn disconnecting() {
    let t = TestDb::new();
    let m = connected(&t, connected(&t, membership(&t)));

    let m = disconnected(&t, m);
    assert!(m.is_connected(t.now()));
    assert_eq!(m.connections, 1);

    let m = disconnected(&t, m);
    assert!(!m.is_connected(t.now()));
    assert_eq!(m.connections, 0);
    assert_eq!(t.read(|c| Membership::find(c, m.id)).connected_at, None);
}

#[test]
fn disconnecting_resets_stale_connection_count() {
    let t = TestDb::new();
    let m = connected(&t, connected(&t, membership(&t)));
    assert_eq!(m.connections, 2);

    t.travel(TTL_PLUS_ONE);
    let m = disconnected(&t, m);
    assert_eq!(m.connections, 0);
}

#[test]
fn refreshing_the_connection() {
    let t = TestDb::new();
    let m = connected(&t, membership(&t));

    t.travel(TTL_PLUS_ONE);
    assert!(!m.is_connected(t.now()));

    let m = run(&t, m, |m, tx| m.refresh_connection(tx));
    assert!(m.is_connected(t.now()));
}

#[test]
fn present_marks_read_and_counts_connections() {
    let t = TestDb::new();
    let m = t.write(|tx| {
        tx.conn().execute(
            "UPDATE memberships SET unread_at = '2026-01-01 00:00:00' WHERE id = ?",
            [id("david_watercooler")],
        )?;
        let mut m = Membership::find(tx.conn(), id("david_watercooler"))?;
        m.present(tx)?;
        Ok(m)
    });
    let reloaded = t.read(|c| Membership::find(c, m.id));
    assert_eq!(reloaded.connections, 1);
    assert_eq!(reloaded.unread_at, None);
    assert_eq!(
        reloaded.updated_at, m.updated_at,
        "Membership.connect doesn't touch updated_at"
    );
}

#[test]
fn disconnect_all_resets_connected_memberships() {
    let t = TestDb::new();
    let m = connected(&t, membership(&t));
    t.write(Membership::disconnect_all);
    let reloaded = t.read(|c| Membership::find(c, m.id));
    assert_eq!((reloaded.connections, reloaded.connected_at), (0, None));
}

#[test]
fn removing_a_membership_resets_the_users_connections() {
    let t = TestDb::new();
    let m = membership(&t);
    t.write(move |tx| m.destroy(tx));
    assert_eq!(
        t.events(),
        vec![Event::DisconnectUser {
            user_id: id("david"),
            reconnect: true
        }]
    );
}

#[test]
fn reading_is_a_noop_when_already_read() {
    let t = TestDb::new();
    let m = membership(&t);
    let before = m.updated_at;
    let m = run(&t, m, |m, tx| m.read(tx));
    assert_eq!(t.read(|c| Membership::find(c, m.id)).updated_at, before);
}

#[test]
fn unread_direct_rooms_count_others_messages_since_they_last_read_them() {
    let t = TestDb::new();
    let david = id("david");
    assert!(t.read(move |c| Membership::unread_direct_counts(c, david)).is_empty(), "nothing unread");
    let room_id: i64 = t.read(move |c| {
        Ok(c.query_row(
            r#"SELECT "memberships"."room_id" FROM "memberships" INNER JOIN "rooms" ON "rooms"."id" = "memberships"."room_id" WHERE "rooms"."type" = 'Rooms::Direct' AND "memberships"."user_id" = ? LIMIT 1"#,
            [david],
            |row| row.get(0),
        )?)
    });
    t.write(move |tx| {
        tx.conn().execute(r#"UPDATE "memberships" SET "unread_at" = '2030-01-01 00:00:00.000000', "last_read_at" = '2000-01-01 00:00:00.000000' WHERE "room_id" = ? AND "user_id" = ?"#, [room_id, david])?;
        Ok(())
    });
    let expected: i64 = t.read(move |c| Ok(c.query_row(r#"SELECT COUNT(*) FROM "messages" WHERE "room_id" = ? AND "creator_id" != ?"#, [room_id, david], |row| row.get(0))?));
    let counts = t.read(move |c| Membership::unread_direct_counts(c, david));
    if expected > 0 {
        assert_eq!(counts, vec![(room_id, expected)]);
    } else {
        assert!(counts.is_empty());
    }

    // Without a last read time (memberships from before this app kept one), only the latest counts.
    t.write(move |tx| {
        tx.conn().execute(r#"UPDATE "memberships" SET "last_read_at" = NULL, "unread_at" = (SELECT MAX("created_at") FROM "messages" WHERE "room_id" = ? AND "creator_id" != ?) WHERE "room_id" = ? AND "user_id" = ?"#, [room_id, david, room_id, david])?;
        Ok(())
    });
    if expected > 0 {
        assert_eq!(t.read(move |c| Membership::unread_direct_counts(c, david)), vec![(room_id, 1)]);
    }
}

#[test]
fn marking_unread_counts_from_that_message_and_stays_until_read() {
    use crate::{Message, NewMessage};
    let t = TestDb::new();
    let david = id("david");
    let (room_id, other): (i64, i64) = t.read(move |c| {
        Ok(c.query_row(
            r#"SELECT "mine"."room_id", "theirs"."user_id" FROM "memberships" AS "mine" INNER JOIN "rooms" ON "rooms"."id" = "mine"."room_id"
               INNER JOIN "memberships" AS "theirs" ON "theirs"."room_id" = "mine"."room_id" AND "theirs"."user_id" != "mine"."user_id"
               WHERE "rooms"."type" = 'Rooms::Direct' AND "mine"."user_id" = ? LIMIT 1"#,
            [david],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    });
    for n in 0..2 {
        let attributes = NewMessage { room_id, creator_id: other, client_message_id: Some(format!("unread-{n}")), body: Some(format!("<div>Unread {n}</div>")), attachment_blob_id: None, thread_parent_id: None };
        t.write(move |tx| Message::create(tx, attributes));
    }
    let others: Vec<Message> = t.read(move |c| Ok(Message::last_page(c, room_id)?.into_iter().filter(|m| m.creator_id == other).collect()));
    let others: Vec<Message> = others[others.len() - 2..].to_vec();
    let from = others[0].clone();
    assert!(t.write(move |tx| Membership::mark_unread_from(tx, david, &from)));
    let counts = t.read(move |c| Membership::unread_direct_counts(c, david));
    assert_eq!(counts, vec![(room_id, others.len() as i64)], "from that message on");

    // Being in the room (refreshing the connection) doesn't move it; reading does.
    let mut membership = t.read(move |c| Membership::find_by_room_and_user(c, room_id, david)).unwrap();
    let mut refreshed = membership.clone();
    t.write(move |tx| refreshed.refresh_connection(tx));
    assert_eq!(t.read(move |c| Membership::unread_direct_counts(c, david)).len(), 1);
    t.write(move |tx| membership.read(tx));
    assert!(t.read(move |c| Membership::unread_direct_counts(c, david)).is_empty());
}
