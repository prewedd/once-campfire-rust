//! Paused notifications (plans/redesign.md): Slack's "Pause notifications" for a while. While
//! paused, nothing is pushed to someone's devices; the rooms still go unread. A table of this
//! app's own, one row per person; a pause past its time is no pause.
use rusqlite::{Connection, params};

use crate::database::Tx;
use crate::error::Result;
use crate::sql::{CachedStatements, query_all, query_one};

/// Created on boot when missing; the Rails app never reads it.
pub const CREATE_TABLE: &str = r#"CREATE TABLE IF NOT EXISTS "notification_pauses" (
    "user_id" integer PRIMARY KEY NOT NULL,
    "until" integer NOT NULL
)"#;

pub struct NotificationPause;

impl NotificationPause {
    /// Pauses someone's notifications until `until` (seconds since the Unix epoch).
    pub fn pause(tx: &mut Tx<'_>, user_id: i64, until: i64) -> Result<()> {
        tx.conn().execute_cached(
            r#"INSERT INTO "notification_pauses" ("user_id", "until") VALUES (?, ?) ON CONFLICT ("user_id") DO UPDATE SET "until" = excluded."until""#,
            params![user_id, until],
        )?;
        Ok(())
    }

    pub fn resume(tx: &mut Tx<'_>, user_id: i64) -> Result<()> {
        tx.conn().execute_cached(r#"DELETE FROM "notification_pauses" WHERE "user_id" = ?"#, [user_id])?;
        Ok(())
    }

    /// Until when theirs are paused, if they are at `now`.
    pub fn until(conn: &Connection, user_id: i64, now: i64) -> Result<Option<i64>> {
        query_one(conn, r#"SELECT "until" FROM "notification_pauses" WHERE "user_id" = ? AND "until" > ?"#, params![user_id, now], |row| row.get(0))
    }

    /// Everyone paused at `now`.
    pub fn paused_user_ids(conn: &Connection, now: i64) -> Result<Vec<i64>> {
        query_all(conn, r#"SELECT "user_id" FROM "notification_pauses" WHERE "until" > ?"#, [now], |row| row.get(0))
    }
}
