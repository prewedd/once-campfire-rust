//! The schema exactly as `bin/rails db:prepare` creates it on a fresh database, and the
//! connection settings from `reference/config/database.yml` plus the sqlite3 adapter's
//! `DEFAULT_PRAGMAS`.
//!
//! `schema.sql` is the `sqlite_master` of a database the reference app created with
//! `db:prepare` (loading `reference/db/schema.rb`), minus the objects SQLite derives on its
//! own (FTS5 shadow tables, `sqlite_sequence`, autoindexes). A fresh `db:prepare` loads
//! `schema.rb`, so columns come out in alphabetical order; databases that were migrated keep
//! migration order. All queries in this crate name their columns, so both work.

use rusqlite::{Connection, OptionalExtension, params};
use sha1::{Digest, Sha1};

use crate::error::{Error, Result};
use crate::time::{Clock, Timestamp};

pub const SCHEMA_SQL: &str = include_str!("schema.sql");

/// Every migration in `reference/db/migrate`, oldest first.
pub const MIGRATION_VERSIONS: &[&str] = &[
    "20231215043540",
    "20231220143106",
    "20240110071740",
    "20240115124901",
    "20240130003150",
    "20240130213001",
    "20240131105830",
    "20240209110503",
    "20250825100957",
    "20250825100958",
    "20250825100959",
    "20251126092013",
    "20251126115722",
    "20251126130131",
    "20251212154340",
];

/// SHA1 of `reference/db/schema.rb`, which `db:schema:load` records in `ar_internal_metadata`.
pub const SCHEMA_SHA1: &str = "f75da8dad38bfb179ffd757bd7a7c2b3f818bc29";

/// Columns this app adds to the Rails schema, as (table, column, definition), added on boot when
/// missing. Nullable and without constraints, so the Rails image (which ignores them) still runs.
pub const ADDED_COLUMNS: &[(&str, &str, &str)] = &[
    // Threads (plans/threads.md): a reply's top-level message.
    ("messages", "thread_parent_id", "integer"),
    // The sidebar's channel order, per person (plans/redesign.md): NULL keeps the alphabetical place.
    ("memberships", "position", "integer"),
    // A channel's topic, shown in its header (plans/redesign.md).
    ("rooms", "topic", "varchar"),
];

/// Indexes this app adds to the Rails schema, created on boot when missing (new and existing
/// databases alike). Additive only, so the database still works with the Rails image.
pub const ADDITIONS: &[&str] = &[
    // Scheduled messages (plans/scheduled-messages.md): a table of this app's own.
    crate::models::scheduled_message::CREATE_TABLE,
    crate::models::scheduled_message::CREATE_INDEX,
    // A room's messages are paged by `created_at` (`last_page`, `page_before`, `page_after`), and
    // with only `index_messages_on_room_id` every room page sorted the room's whole history:
    // 60 ms at 236k messages, against 0.02 ms with this index.
    r#"CREATE INDEX IF NOT EXISTS "index_messages_on_room_id_and_created_at" ON "messages" ("room_id", "created_at")"#,
    // A thread's replies, paged like a room's, and counted for its parent's summary.
    r#"CREATE INDEX IF NOT EXISTS "index_messages_on_thread_parent_id_and_created_at" ON "messages" ("thread_parent_id", "created_at")"#,
];

/// `timeout: 5000` in `config/database.yml`.
pub const BUSY_TIMEOUT_MS: u64 = 5000;

/// Applies the per-connection settings Rails applies (`SQLite3Adapter#configure_connection`).
pub fn configure_connection(conn: &Connection) -> Result<()> {
    conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "journal_mode", "wal")?;
    conn.pragma_update(None, "synchronous", "normal")?;
    conn.pragma_update(None, "mmap_size", 134_217_728)?;
    conn.pragma_update(None, "journal_size_limit", 67_108_864)?;
    conn.pragma_update(None, "cache_size", 2000)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prepared {
    /// The database was empty; the schema was loaded.
    Loaded,
    /// The schema was already current.
    UpToDate,
}

/// `bin/rails db:prepare`: loads the schema into an empty database, or verifies an
/// existing one is fully migrated. We don't port migrations, so a database with pending
/// migrations is an error (boot the Rails image once to migrate it). Then adds [`ADDITIONS`].
pub fn prepare(conn: &mut Connection, environment: &str, clock: &dyn Clock) -> Result<Prepared> {
    let prepared = if table_exists(conn, "schema_migrations")? {
        let pending = pending_migrations(conn)?;
        if !pending.is_empty() {
            return Err(Error::Other(format!(
                "pending migrations: {}",
                pending.join(", ")
            )));
        }
        Prepared::UpToDate
    } else {
        load_schema(conn, environment, clock)?;
        Prepared::Loaded
    };
    add_missing_columns(conn)?;
    for addition in ADDITIONS {
        conn.execute_batch(addition)?;
    }
    Ok(prepared)
}

/// [`ADDED_COLUMNS`] that aren't there yet.
fn add_missing_columns(conn: &Connection) -> Result<()> {
    for (table, column, definition) in ADDED_COLUMNS {
        if !column_exists(conn, table, column)? {
            conn.execute_batch(&format!(
                r#"ALTER TABLE "{table}" ADD COLUMN "{column}" {definition}"#
            ))?;
        }
    }
    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    Ok(conn
        .prepare("SELECT 1 FROM pragma_table_info(?) WHERE name = ?")?
        .exists([table, column])?)
}

pub fn pending_migrations(conn: &Connection) -> Result<Vec<&'static str>> {
    let mut stmt = conn.prepare("SELECT 1 FROM schema_migrations WHERE version = ?")?;
    let mut pending = Vec::new();
    for version in MIGRATION_VERSIONS {
        if !stmt.exists([version])? {
            pending.push(*version);
        }
    }
    Ok(pending)
}

fn load_schema(conn: &mut Connection, environment: &str, clock: &dyn Clock) -> Result<()> {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute_batch(SCHEMA_SQL)?;

    // `assume_migrated_upto_version` inserts the current version, then the rest newest first.
    for version in MIGRATION_VERSIONS.iter().rev() {
        tx.execute(
            r#"INSERT INTO "schema_migrations" ("version") VALUES (?)"#,
            [version],
        )?;
    }

    set_internal_metadata(&tx, "environment", environment, clock.now())?;
    set_internal_metadata(&tx, "schema_sha1", SCHEMA_SHA1, clock.now())?;
    tx.commit()?;
    Ok(())
}

fn set_internal_metadata(conn: &Connection, key: &str, value: &str, now: Timestamp) -> Result<()> {
    let existing: Option<String> = conn
        .query_row(
            r#"SELECT "value" FROM "ar_internal_metadata" WHERE "key" = ?"#,
            [key],
            |r| r.get(0),
        )
        .optional()?;
    match existing {
        None => {
            conn.execute(
                r#"INSERT INTO "ar_internal_metadata" ("key", "value", "created_at", "updated_at") VALUES (?, ?, ?, ?)"#,
                params![key, value, now, now],
            )?;
        }
        Some(current) if current != value => {
            conn.execute(
                r#"UPDATE "ar_internal_metadata" SET "value" = ?, "updated_at" = ? WHERE "key" = ?"#,
                params![value, now, key],
            )?;
        }
        Some(_) => {}
    }
    Ok(())
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    Ok(conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?")?
        .exists([name])?)
}

/// SHA1 hex of a schema file, as Rails computes it for `schema_sha1`.
pub fn schema_sha1(contents: &[u8]) -> String {
    hex::encode(Sha1::digest(contents))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::SystemClock;

    const REFERENCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../reference/db");

    #[test]
    fn schema_sha1_matches_reference_schema_rb() {
        let contents = std::fs::read(format!("{REFERENCE}/schema.rb")).unwrap();
        assert_eq!(schema_sha1(&contents), SCHEMA_SHA1);
    }

    #[test]
    fn migration_versions_match_reference_migrations() {
        let mut versions: Vec<String> = std::fs::read_dir(format!("{REFERENCE}/migrate"))
            .unwrap()
            .map(|e| {
                e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .split('_')
                    .next()
                    .unwrap()
                    .to_string()
            })
            .collect();
        versions.sort();
        assert_eq!(versions, MIGRATION_VERSIONS);
    }

    #[test]
    fn prepare_loads_then_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        assert_eq!(
            prepare(&mut conn, "production", &SystemClock).unwrap(),
            Prepared::Loaded
        );
        assert_eq!(
            prepare(&mut conn, "production", &SystemClock).unwrap(),
            Prepared::UpToDate
        );

        let first: String = conn
            .query_row(
                "SELECT version FROM schema_migrations ORDER BY rowid LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(first, "20251212154340");
        let sha: String = conn
            .query_row(
                "SELECT value FROM ar_internal_metadata WHERE key = 'schema_sha1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sha, SCHEMA_SHA1);
        conn.execute(
            "INSERT INTO message_search_index(rowid, body) VALUES (1, 'running dogs')",
            [],
        )
        .unwrap();
        let hit: i64 = conn
            .query_row(
                "SELECT rowid FROM message_search_index WHERE body MATCH 'run'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hit, 1, "porter tokenizer");
    }

    fn query_plan(conn: &Connection, sql: &str) -> String {
        let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(3)).unwrap();
        rows.map(|r| r.unwrap()).collect::<Vec<_>>().join("; ")
    }

    #[test]
    fn prepare_adds_the_room_paging_index_to_new_and_existing_databases() {
        let last_page = r#"SELECT * FROM "messages" WHERE "room_id" = 1 ORDER BY "created_at" DESC LIMIT 40"#;
        let mut conn = Connection::open_in_memory().unwrap();
        prepare(&mut conn, "production", &SystemClock).unwrap();
        let plan = query_plan(&conn, last_page);
        assert!(plan.contains("index_messages_on_room_id_and_created_at") && !plan.contains("TEMP B-TREE"), "{plan}");

        // A database the Rails app created doesn't have it until the app boots on it.
        conn.execute_batch(r#"DROP INDEX "index_messages_on_room_id_and_created_at""#).unwrap();
        assert!(query_plan(&conn, last_page).contains("TEMP B-TREE"));
        assert_eq!(prepare(&mut conn, "production", &SystemClock).unwrap(), Prepared::UpToDate);
        assert!(!query_plan(&conn, last_page).contains("TEMP B-TREE"));
    }

    #[test]
    fn prepare_adds_the_thread_column_to_databases_the_rails_app_made() {
        let mut conn = Connection::open_in_memory().unwrap();
        load_schema(&mut conn, "production", &SystemClock).unwrap();
        assert!(!column_exists(&conn, "messages", "thread_parent_id").unwrap());
        conn.execute_batch(
            r#"PRAGMA foreign_keys = OFF;
               INSERT INTO "rooms" ("id", "created_at", "creator_id", "type", "updated_at") VALUES (1, '2026-01-01', 1, 'Rooms::Open', '2026-01-01');
               INSERT INTO "messages" ("client_message_id", "created_at", "creator_id", "room_id", "updated_at") VALUES ('a', '2026-01-01', 1, 1, '2026-01-01');"#,
        )
        .unwrap();

        assert_eq!(
            prepare(&mut conn, "production", &SystemClock).unwrap(),
            Prepared::UpToDate
        );
        assert!(column_exists(&conn, "messages", "thread_parent_id").unwrap());
        let parent: Option<i64> = conn
            .query_row(r#"SELECT "thread_parent_id" FROM "messages""#, [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(parent, None);
        let plan = query_plan(
            &conn,
            r#"SELECT * FROM "messages" WHERE "thread_parent_id" = 1 ORDER BY "created_at""#,
        );
        assert!(
            plan.contains("index_messages_on_thread_parent_id_and_created_at"),
            "{plan}"
        );

        // And again on the next boot, without complaint.
        assert_eq!(
            prepare(&mut conn, "production", &SystemClock).unwrap(),
            Prepared::UpToDate
        );
    }

    #[test]
    fn prepare_reports_pending_migrations() {
        let mut conn = Connection::open_in_memory().unwrap();
        prepare(&mut conn, "production", &SystemClock).unwrap();
        conn.execute(
            "DELETE FROM schema_migrations WHERE version = '20251212154340'",
            [],
        )
        .unwrap();
        assert!(prepare(&mut conn, "production", &SystemClock).is_err());
    }
}
