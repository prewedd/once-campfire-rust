//! Pinned messages (plans/redesign.md): a channel's messages kept at hand, as Slack's pins. Any
//! member may pin or unpin. A table of this app's own; a deleted message's pin goes unlisted.
use rusqlite::{Connection, params};

use crate::database::Tx;
use crate::error::Result;
use crate::models::Message;
use crate::sql::{CachedStatements, query_all};

/// Created on boot when missing; the Rails app never reads it.
pub const CREATE_TABLE: &str = r#"CREATE TABLE IF NOT EXISTS "pinned_messages" (
    "room_id" integer NOT NULL,
    "message_id" integer NOT NULL,
    "user_id" integer NOT NULL,
    "created_at" integer NOT NULL,
    PRIMARY KEY ("room_id", "message_id")
)"#;

pub struct PinnedMessage;

impl PinnedMessage {
    /// The most a channel lists.
    pub const LIMIT: i64 = 100;

    /// Pins it in its room, if `user_id` is a member there. `Some(room_id)` when pinned (or it already
    /// was), `None` when they can't (or there's no such message).
    pub fn pin(tx: &mut Tx<'_>, user_id: i64, message_id: i64, now: i64) -> Result<Option<i64>> {
        let Some(room_id) = Self::member_room(tx.conn(), user_id, message_id)? else { return Ok(None) };
        tx.conn().execute_cached(
            r#"INSERT OR IGNORE INTO "pinned_messages" ("room_id", "message_id", "user_id", "created_at") VALUES (?, ?, ?, ?)"#,
            params![room_id, message_id, user_id, now],
        )?;
        Ok(Some(room_id))
    }

    pub fn unpin(tx: &mut Tx<'_>, user_id: i64, message_id: i64) -> Result<Option<i64>> {
        let Some(room_id) = Self::member_room(tx.conn(), user_id, message_id)? else { return Ok(None) };
        tx.conn().execute_cached(r#"DELETE FROM "pinned_messages" WHERE "room_id" = ? AND "message_id" = ?"#, params![room_id, message_id])?;
        Ok(Some(room_id))
    }

    /// The message's room, when `user_id` is a member of it.
    fn member_room(conn: &Connection, user_id: i64, message_id: i64) -> Result<Option<i64>> {
        crate::sql::query_one(
            conn,
            r#"SELECT "messages"."room_id" FROM "messages" INNER JOIN "memberships" ON "memberships"."room_id" = "messages"."room_id" WHERE "messages"."id" = ? AND "memberships"."user_id" = ?"#,
            params![message_id, user_id],
            |row| row.get(0),
        )
    }

    /// A room's pinned messages, most recently pinned first.
    pub fn messages(conn: &Connection, room_id: i64) -> Result<Vec<Message>> {
        query_all(
            conn,
            r#"SELECT "messages".* FROM "pinned_messages" INNER JOIN "messages" ON "messages"."id" = "pinned_messages"."message_id"
               WHERE "pinned_messages"."room_id" = ? ORDER BY "pinned_messages"."created_at" DESC, "pinned_messages"."message_id" DESC LIMIT ?"#,
            params![room_id, Self::LIMIT],
            Message::from_row,
        )
    }

    /// Their ids, in the same order.
    pub fn ids(conn: &Connection, room_id: i64) -> Result<Vec<i64>> {
        Ok(Self::messages(conn, room_id)?.into_iter().map(|message| message.id).collect())
    }
}
