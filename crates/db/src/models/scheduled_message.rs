//! Scheduled messages (plans/scheduled-messages.md): a message someone wrote to be posted later.
//!
//! Posting one creates the message and marks the schedule sent in the same transaction, so each is
//! posted exactly once: never twice, and not lost to a restart (one that was due while the app was
//! down goes out when it's back).
use rusqlite::{Connection, Row, params};

use crate::database::Tx;
use crate::error::{OptionalExt, Result};
use crate::models::{Message, NewMessage};
use crate::sql::{CachedStatements, query_all, query_one};

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledMessage {
    pub id: i64,
    pub room_id: i64,
    pub creator_id: i64,
    pub thread_parent_id: Option<i64>,
    /// The message's body, as `Message::create` takes it (canonical rich text HTML).
    pub body: String,
    /// When to post it, in seconds since the Unix epoch.
    pub send_at: i64,
    pub created_at: i64,
    pub message_id: Option<i64>,
}

pub struct NewScheduledMessage {
    pub room_id: i64,
    pub creator_id: i64,
    pub thread_parent_id: Option<i64>,
    pub body: String,
    pub send_at: i64,
}

/// Created on boot when missing; the Rails app never reads it.
pub const CREATE_TABLE: &str = r#"CREATE TABLE IF NOT EXISTS "scheduled_messages" (
    "id" integer PRIMARY KEY AUTOINCREMENT NOT NULL,
    "room_id" integer NOT NULL,
    "creator_id" integer NOT NULL,
    "thread_parent_id" integer,
    "body" text NOT NULL,
    "send_at" integer NOT NULL,
    "created_at" integer NOT NULL,
    "message_id" integer
)"#;
pub const CREATE_INDEX: &str =
    r#"CREATE INDEX IF NOT EXISTS "index_scheduled_messages_pending" ON "scheduled_messages" ("send_at") WHERE "message_id" IS NULL"#;

impl ScheduledMessage {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            room_id: row.get("room_id")?,
            creator_id: row.get("creator_id")?,
            thread_parent_id: row.get("thread_parent_id")?,
            body: row.get("body")?,
            send_at: row.get("send_at")?,
            created_at: row.get("created_at")?,
            message_id: row.get("message_id")?,
        })
    }

    pub fn create(tx: &mut Tx<'_>, new: NewScheduledMessage, now: i64) -> Result<Self> {
        tx.conn().execute_cached(
            r#"INSERT INTO "scheduled_messages" ("room_id", "creator_id", "thread_parent_id", "body", "send_at", "created_at") VALUES (?, ?, ?, ?, ?, ?)"#,
            params![new.room_id, new.creator_id, new.thread_parent_id, new.body, new.send_at, now],
        )?;
        let id = tx.conn().last_insert_rowid();
        Self::find(tx.conn(), id)
    }

    pub fn find(conn: &Connection, id: i64) -> Result<Self> {
        query_one(conn, r#"SELECT * FROM "scheduled_messages" WHERE "id" = ?"#, [id], Self::from_row)?.or_not_found("ScheduledMessage")
    }

    /// This person's, not yet posted, soonest first: in one room, or all of them.
    pub fn pending_for(conn: &Connection, creator_id: i64, room_id: Option<i64>) -> Result<Vec<Self>> {
        query_all(
            conn,
            r#"SELECT * FROM "scheduled_messages" WHERE "creator_id" = ? AND "message_id" IS NULL AND (? IS NULL OR "room_id" = ?) ORDER BY "send_at", "id""#,
            params![creator_id, room_id, room_id],
            Self::from_row,
        )
    }

    /// The ids of those due by `now`, oldest first.
    pub fn due(conn: &Connection, now: i64, limit: i64) -> Result<Vec<i64>> {
        query_all(
            conn,
            r#"SELECT "id" FROM "scheduled_messages" WHERE "message_id" IS NULL AND "send_at" <= ? ORDER BY "send_at", "id" LIMIT ?"#,
            params![now, limit],
            |row| row.get(0),
        )
    }

    pub fn reschedule(&mut self, tx: &mut Tx<'_>, send_at: i64, body: Option<String>) -> Result<()> {
        let body = body.unwrap_or_else(|| self.body.clone());
        tx.conn().execute_cached(
            r#"UPDATE "scheduled_messages" SET "send_at" = ?, "body" = ? WHERE "id" = ? AND "message_id" IS NULL"#,
            params![send_at, body, self.id],
        )?;
        self.send_at = send_at;
        self.body = body;
        Ok(())
    }

    pub fn destroy(&self, tx: &mut Tx<'_>) -> Result<()> {
        tx.conn().execute_cached(r#"DELETE FROM "scheduled_messages" WHERE "id" = ? AND "message_id" IS NULL"#, [self.id])?;
        Ok(())
    }

    /// Posts it, if it's still waiting and its writer may still post in the room: the message, and
    /// the schedule marked sent, together. `None` when there was nothing to post (already posted,
    /// deleted, or they've left the room, in which case it's dropped).
    pub fn post(tx: &mut Tx<'_>, id: i64) -> Result<Option<Message>> {
        let Some(scheduled) = query_one(tx.conn(), r#"SELECT * FROM "scheduled_messages" WHERE "id" = ? AND "message_id" IS NULL"#, [id], Self::from_row)? else {
            return Ok(None);
        };
        let allowed: bool = tx.conn().query_row(
            r#"SELECT EXISTS (SELECT 1 FROM "memberships" INNER JOIN "users" ON "users"."id" = "memberships"."user_id" WHERE "memberships"."room_id" = ? AND "memberships"."user_id" = ? AND "users"."status" = 0)"#,
            params![scheduled.room_id, scheduled.creator_id],
            |row| row.get(0),
        )?;
        if !allowed {
            tx.conn().execute_cached(r#"DELETE FROM "scheduled_messages" WHERE "id" = ?"#, [id])?;
            return Ok(None);
        }
        // A reply whose thread went in the meantime goes to the room instead.
        let thread_parent_id = match scheduled.thread_parent_id {
            Some(parent) if Message::find_by_id(tx.conn(), parent)?.is_some() => Some(parent),
            _ => None,
        };
        let message = Message::create(
            tx,
            NewMessage {
                room_id: scheduled.room_id,
                creator_id: scheduled.creator_id,
                client_message_id: None,
                body: Some(scheduled.body.clone()),
                attachment_blob_id: None,
                thread_parent_id,
            },
        )?;
        tx.conn().execute_cached(r#"UPDATE "scheduled_messages" SET "message_id" = ? WHERE "id" = ?"#, params![message.id, id])?;
        Ok(Some(message))
    }
}
