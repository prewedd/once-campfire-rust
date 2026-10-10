//! Saved for later (plans/redesign.md): messages someone keeps to come back to, as Slack's "Later".
//! A table of this app's own. Only messages they can reach are saved or listed, so leaving a room
//! hides its messages here too.
use rusqlite::{Connection, params};

use crate::database::Tx;
use crate::error::Result;
use crate::models::Message;
use crate::sql::{CachedStatements, query_all};

/// Created on boot when missing; the Rails app never reads it.
pub const CREATE_TABLE: &str = r#"CREATE TABLE IF NOT EXISTS "saved_messages" (
    "user_id" integer NOT NULL,
    "message_id" integer NOT NULL,
    "created_at" integer NOT NULL,
    PRIMARY KEY ("user_id", "message_id")
)"#;

const REACHABLE: &str = r#"INNER JOIN "messages" ON "messages"."id" = "saved_messages"."message_id"
    INNER JOIN "memberships" ON "memberships"."room_id" = "messages"."room_id" AND "memberships"."user_id" = "saved_messages"."user_id""#;

pub struct SavedMessage;

impl SavedMessage {
    /// The most listed.
    pub const LIMIT: i64 = 200;

    /// Saves it, if they can reach it; `false` when they can't (or there's no such message).
    pub fn save(tx: &mut Tx<'_>, user_id: i64, message_id: i64, now: i64) -> Result<bool> {
        let reachable: bool = tx.conn().query_row(
            r#"SELECT EXISTS (SELECT 1 FROM "messages" INNER JOIN "memberships" ON "memberships"."room_id" = "messages"."room_id" WHERE "messages"."id" = ? AND "memberships"."user_id" = ?)"#,
            params![message_id, user_id],
            |row| row.get(0),
        )?;
        if reachable {
            tx.conn().execute_cached(
                r#"INSERT OR IGNORE INTO "saved_messages" ("user_id", "message_id", "created_at") VALUES (?, ?, ?)"#,
                params![user_id, message_id, now],
            )?;
        }
        Ok(reachable)
    }

    pub fn unsave(tx: &mut Tx<'_>, user_id: i64, message_id: i64) -> Result<()> {
        tx.conn().execute_cached(r#"DELETE FROM "saved_messages" WHERE "user_id" = ? AND "message_id" = ?"#, params![user_id, message_id])?;
        Ok(())
    }

    /// The ids of theirs they can still reach, most recently saved first.
    pub fn ids(conn: &Connection, user_id: i64) -> Result<Vec<i64>> {
        query_all(
            conn,
            &format!(r#"SELECT "saved_messages"."message_id" FROM "saved_messages" {REACHABLE} WHERE "saved_messages"."user_id" = ? ORDER BY "saved_messages"."created_at" DESC, "saved_messages"."message_id" DESC LIMIT ?"#),
            params![user_id, Self::LIMIT],
            |row| row.get(0),
        )
    }

    /// Those messages, in the same order.
    pub fn messages(conn: &Connection, user_id: i64) -> Result<Vec<Message>> {
        query_all(
            conn,
            &format!(r#"SELECT {} FROM "saved_messages" {REACHABLE} WHERE "saved_messages"."user_id" = ? ORDER BY "saved_messages"."created_at" DESC, "saved_messages"."message_id" DESC LIMIT ?"#, crate::models::message::message_columns!()),
            params![user_id, Self::LIMIT],
            Message::from_row,
        )
    }
}
