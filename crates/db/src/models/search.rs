//! `reference/app/models/search.rb`

use rusqlite::{Connection, Row, params};

use crate::database::Tx;
use crate::error::Result;
use crate::sql::{self, CachedStatements, placeholders, query_all, query_one};
use crate::time::Timestamp;

/// How many recent searches `trim_recent_searches` keeps.
pub const RECENT_SEARCHES: i64 = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct Search {
    pub id: i64,
    pub user_id: i64,
    pub query: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Search {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            user_id: row.get("user_id")?,
            query: row.get("query")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    /// `user.searches.ordered`: most recent first.
    pub fn ordered_for_user(conn: &Connection, user_id: i64) -> Result<Vec<Self>> {
        query_all(
            conn,
            r#"SELECT "searches".* FROM "searches" WHERE "searches"."user_id" = ? ORDER BY "searches"."updated_at" DESC"#,
            [user_id],
            Self::from_row,
        )
    }

    pub fn count(conn: &Connection) -> Result<i64> {
        sql::count(conn, r#"SELECT COUNT(*) FROM "searches""#, [])
    }

    pub fn count_for_user(conn: &Connection, user_id: i64) -> Result<i64> {
        sql::count(conn, r#"SELECT COUNT(*) FROM "searches" WHERE "searches"."user_id" = ?"#, [user_id])
    }

    /// `user.searches.record(query)`: `find_or_create_by(query:).touch`. Creating trims the
    /// user's searches to the ten most recent.
    pub fn record(tx: &mut Tx<'_>, user_id: i64, query: &str) -> Result<Self> {
        let existing = query_one(
            tx.conn(),
            r#"SELECT "searches".* FROM "searches" WHERE "searches"."user_id" = ? AND "searches"."query" = ? LIMIT 1"#,
            params![user_id, query],
            Self::from_row,
        )?;
        let mut search = match existing {
            Some(search) => search,
            None => Self::create(tx, user_id, query)?,
        };
        let now = tx.now();
        tx.conn().execute_cached(r#"UPDATE "searches" SET "updated_at" = ? WHERE "searches"."id" = ?"#, params![now, search.id])?;
        search.updated_at = now;
        Ok(search)
    }

    fn create(tx: &mut Tx<'_>, user_id: i64, query: &str) -> Result<Self> {
        let now = tx.now();
        let id: i64 = tx.conn().query_row_cached(
            r#"INSERT INTO "searches" ("created_at", "query", "updated_at", "user_id") VALUES (?, ?, ?, ?) RETURNING "id""#,
            params![now, query, now, user_id],
            |r| r.get(0),
        )?;
        trim_recent_searches(tx, user_id)?;
        Ok(Self { id, user_id, query: query.into(), created_at: now, updated_at: now })
    }

    /// `user.searches.destroy_all`
    pub fn destroy_all_for_user(tx: &mut Tx<'_>, user_id: i64) -> Result<()> {
        let ids: Vec<i64> =
            query_all(tx.conn(), r#"SELECT "searches"."id" FROM "searches" WHERE "searches"."user_id" = ?"#, [user_id], |r| r.get(0))?;
        for id in ids {
            tx.conn().execute_cached(r#"DELETE FROM "searches" WHERE "searches"."id" = ?"#, [id])?;
        }
        Ok(())
    }
}

/// `user.searches.excluding(user.searches.ordered.limit(10)).destroy_all`
fn trim_recent_searches(tx: &Tx<'_>, user_id: i64) -> Result<()> {
    let keep: Vec<i64> = query_all(
        tx.conn(),
        r#"SELECT "searches"."id" FROM "searches" WHERE "searches"."user_id" = ? ORDER BY "searches"."updated_at" DESC LIMIT ?"#,
        params![user_id, RECENT_SEARCHES],
        |r| r.get(0),
    )?;
    let sql = format!(
        r#"SELECT "searches"."id" FROM "searches" WHERE "searches"."user_id" = ? AND "searches"."id" NOT IN ({})"#,
        placeholders(keep.len().max(1))
    );
    let mut values: Vec<i64> = vec![user_id];
    if keep.is_empty() {
        values.push(0);
    } else {
        values.extend(keep);
    }
    let doomed: Vec<i64> = query_all(tx.conn(), &sql, rusqlite::params_from_iter(values), |r| r.get(0))?;
    for id in doomed {
        tx.conn().execute_cached(r#"DELETE FROM "searches" WHERE "searches"."id" = ?"#, [id])?;
    }
    Ok(())
}
