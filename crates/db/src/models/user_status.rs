//! Custom statuses (plans/redesign.md): an emoji and a few words, as Slack's "Update your status".
//! A table of this app's own, one row per person, deleted when they clear it. A status past its
//! `clears_at` reads as none, so nothing has to sweep them.
use rusqlite::{Connection, Row, params};

use crate::database::Tx;
use crate::error::Result;
use crate::sql::{CachedStatements, query_all, query_one};

#[derive(Debug, Clone, PartialEq)]
pub struct UserStatus {
    pub user_id: i64,
    pub emoji: String,
    pub text: String,
    /// When it clears itself, in seconds since the Unix epoch; `None` keeps it until cleared.
    pub clears_at: Option<i64>,
}

/// Created on boot when missing; the Rails app never reads it.
pub const CREATE_TABLE: &str = r#"CREATE TABLE IF NOT EXISTS "user_statuses" (
    "user_id" integer PRIMARY KEY NOT NULL,
    "emoji" varchar NOT NULL,
    "text" varchar NOT NULL,
    "clears_at" integer,
    "updated_at" integer NOT NULL
)"#;

impl UserStatus {
    pub const TEXT_LIMIT: usize = 100;
    pub const EMOJI_LIMIT: usize = 16;
    /// Slack's, for a status with words and no emoji.
    pub const DEFAULT_EMOJI: &str = "💬";

    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self { user_id: row.get("user_id")?, emoji: row.get("emoji")?, text: row.get("text")?, clears_at: row.get("clears_at")? })
    }

    /// Sets someone's status: both parts blank clears it.
    pub fn set(tx: &mut Tx<'_>, user_id: i64, emoji: &str, text: &str, clears_at: Option<i64>, now: i64) -> Result<Option<Self>> {
        let text: String = text.trim().chars().take(Self::TEXT_LIMIT).collect();
        let emoji: String = emoji.trim().chars().take(Self::EMOJI_LIMIT).collect();
        if text.is_empty() && emoji.is_empty() {
            Self::clear(tx, user_id)?;
            return Ok(None);
        }
        let emoji = if emoji.is_empty() { Self::DEFAULT_EMOJI.to_string() } else { emoji };
        tx.conn().execute_cached(
            r#"INSERT INTO "user_statuses" ("user_id", "emoji", "text", "clears_at", "updated_at") VALUES (?, ?, ?, ?, ?)
               ON CONFLICT ("user_id") DO UPDATE SET "emoji" = excluded."emoji", "text" = excluded."text", "clears_at" = excluded."clears_at", "updated_at" = excluded."updated_at""#,
            params![user_id, emoji, text, clears_at, now],
        )?;
        Ok(Some(Self { user_id, emoji, text, clears_at }))
    }

    pub fn clear(tx: &mut Tx<'_>, user_id: i64) -> Result<()> {
        tx.conn().execute_cached(r#"DELETE FROM "user_statuses" WHERE "user_id" = ?"#, [user_id])?;
        Ok(())
    }

    /// Everyone's that hasn't cleared itself by `now`.
    pub fn current(conn: &Connection, now: i64) -> Result<Vec<Self>> {
        query_all(conn, r#"SELECT * FROM "user_statuses" WHERE "clears_at" IS NULL OR "clears_at" > ? ORDER BY "user_id""#, [now], Self::from_row)
    }

    pub fn for_user(conn: &Connection, user_id: i64, now: i64) -> Result<Option<Self>> {
        query_one(conn, r#"SELECT * FROM "user_statuses" WHERE "user_id" = ? AND ("clears_at" IS NULL OR "clears_at" > ?)"#, params![user_id, now], Self::from_row)
    }
}
