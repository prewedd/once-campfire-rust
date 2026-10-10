//! `reference/app/models/membership.rb` and `membership/connectable.rb`.

use jiff::SignedDuration;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::{Connection, Row, params};

use crate::database::Tx;
use crate::error::{OptionalExt, Result};
use crate::models::{Room, User};
use crate::sql::{self, CachedStatements, query_all, query_one};
use crate::time::Timestamp;

/// `enum :involvement, %w[ invisible nothing mentions everything ].index_by(&:itself)`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Involvement {
    Invisible,
    Nothing,
    Mentions,
    Everything,
}

impl Involvement {
    pub fn name(self) -> &'static str {
        match self {
            Involvement::Invisible => "invisible",
            Involvement::Nothing => "nothing",
            Involvement::Mentions => "mentions",
            Involvement::Everything => "everything",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "invisible" => Some(Involvement::Invisible),
            "nothing" => Some(Involvement::Nothing),
            "mentions" => Some(Involvement::Mentions),
            "everything" => Some(Involvement::Everything),
            _ => None,
        }
    }
}

impl ToSql for Involvement {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.name()))
    }
}

impl FromSql for Involvement {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let name = value.as_str()?;
        Involvement::from_name(name)
            .ok_or_else(|| FromSqlError::Other(format!("unknown involvement {name:?}").into()))
    }
}

/// `Membership::Connectable::CONNECTION_TTL`
pub const CONNECTION_TTL: SignedDuration = SignedDuration::from_secs(60);

#[derive(Debug, Clone, PartialEq)]
pub struct Membership {
    pub id: i64,
    pub room_id: i64,
    pub user_id: i64,
    /// Nullable in the schema (default "mentions").
    pub involvement: Option<Involvement>,
    pub unread_at: Option<Timestamp>,
    pub connected_at: Option<Timestamp>,
    pub connections: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Membership {
    pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            room_id: row.get("room_id")?,
            user_id: row.get("user_id")?,
            involvement: row.get("involvement")?,
            unread_at: row.get("unread_at")?,
            connected_at: row.get("connected_at")?,
            connections: row.get("connections")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    pub fn find(conn: &Connection, id: i64) -> Result<Self> {
        query_one(
            conn,
            r#"SELECT * FROM "memberships" WHERE "memberships"."id" = ? LIMIT 1"#,
            [id],
            Self::from_row,
        )?
        .or_not_found("Membership")
    }

    pub fn count(conn: &Connection) -> Result<i64> {
        sql::count(conn, r#"SELECT COUNT(*) FROM "memberships""#, [])
    }

    pub fn for_user(conn: &Connection, user_id: i64) -> Result<Vec<Self>> {
        query_all(
            conn,
            r#"SELECT * FROM "memberships" WHERE "memberships"."user_id" = ?"#,
            [user_id],
            Self::from_row,
        )
    }

    pub fn for_room(conn: &Connection, room_id: i64) -> Result<Vec<Self>> {
        query_all(
            conn,
            r#"SELECT * FROM "memberships" WHERE "memberships"."room_id" = ?"#,
            [room_id],
            Self::from_row,
        )
    }

    /// `room.memberships.find_by(user:)` / `user.memberships.find_by(room_id:)`
    pub fn find_by_room_and_user(
        conn: &Connection,
        room_id: i64,
        user_id: i64,
    ) -> Result<Option<Self>> {
        query_one(
            conn,
            r#"SELECT * FROM "memberships" WHERE "memberships"."room_id" = ? AND "memberships"."user_id" = ? LIMIT 1"#,
            [room_id, user_id],
            Self::from_row,
        )
    }

    /// `user.memberships.visible.with_ordered_room`, each with its room.
    pub fn visible_with_ordered_room(conn: &Connection, user_id: i64) -> Result<Vec<(Self, Room)>> {
        query_all(
            conn,
            r#"SELECT "memberships".*, "rooms"."id" AS r_id, "rooms"."created_at" AS r_created_at, "rooms"."creator_id" AS r_creator_id, "rooms"."name" AS r_name, "rooms"."type" AS r_type, "rooms"."updated_at" AS r_updated_at FROM "memberships" INNER JOIN "rooms" ON "rooms"."id" = "memberships"."room_id" WHERE "memberships"."user_id" = ? AND "memberships"."involvement" != 'invisible' ORDER BY LOWER(rooms.name)"#,
            [user_id],
            |row| Ok((Self::from_row(row)?, room_from_prefixed_row(row)?)),
        )
    }

    /// `user.memberships.with_ordered_room` (invisible included).
    pub fn with_ordered_room(conn: &Connection, user_id: i64) -> Result<Vec<(Self, Room)>> {
        query_all(
            conn,
            r#"SELECT "memberships".*, "rooms"."id" AS r_id, "rooms"."created_at" AS r_created_at, "rooms"."creator_id" AS r_creator_id, "rooms"."name" AS r_name, "rooms"."type" AS r_type, "rooms"."updated_at" AS r_updated_at FROM "memberships" INNER JOIN "rooms" ON "rooms"."id" = "memberships"."room_id" WHERE "memberships"."user_id" = ? ORDER BY LOWER(rooms.name)"#,
            [user_id],
            |row| Ok((Self::from_row(row)?, room_from_prefixed_row(row)?)),
        )
    }

    /// `user.memberships.without_direct_rooms.count`
    pub fn count_without_direct_rooms(conn: &Connection, user_id: i64) -> Result<i64> {
        sql::count(
            conn,
            r#"SELECT COUNT(*) FROM "memberships" INNER JOIN "rooms" "room" ON "room"."id" = "memberships"."room_id" WHERE "memberships"."user_id" = ? AND "room"."type" != 'Rooms::Direct'"#,
            [user_id],
        )
    }

    /// `user.memberships.unread.count`: the push badge.
    pub fn unread_count(conn: &Connection, user_id: i64) -> Result<i64> {
        sql::count(
            conn,
            r#"SELECT COUNT(*) FROM "memberships" WHERE "memberships"."user_id" = ? AND "memberships"."unread_at" IS NOT NULL"#,
            [user_id],
        )
    }

    /// `Membership.connected.exists?(id)`
    pub fn connected_exists(conn: &Connection, id: i64, now: Timestamp) -> Result<bool> {
        sql::exists(
            conn,
            r#"SELECT 1 FROM "memberships" WHERE "memberships"."connected_at" >= ? AND "memberships"."id" = ? LIMIT 1"#,
            params![Self::connection_cutoff(now), id],
        )
    }

    /// `Membership.disconnected.exists?(id)`
    pub fn disconnected_exists(conn: &Connection, id: i64, now: Timestamp) -> Result<bool> {
        sql::exists(
            conn,
            r#"SELECT 1 FROM "memberships" WHERE ("memberships"."connected_at" IS NULL OR "memberships"."connected_at" < ?) AND "memberships"."id" = ? LIMIT 1"#,
            params![Self::connection_cutoff(now), id],
        )
    }

    /// `CONNECTION_TTL.ago`
    pub fn connection_cutoff(now: Timestamp) -> Timestamp {
        now.ago(CONNECTION_TTL)
    }

    pub fn room(&self, conn: &Connection) -> Result<Room> {
        Room::find(conn, self.room_id)
    }

    pub fn user(&self, conn: &Connection) -> Result<User> {
        User::find(conn, self.user_id)
    }

    // Involvement and read state

    pub fn involved_in(&self, involvement: Involvement) -> bool {
        self.involvement == Some(involvement)
    }

    /// `update!(involvement:)`
    pub fn update_involvement(&mut self, tx: &mut Tx<'_>, involvement: impl Into<Option<Involvement>>) -> Result<()> {
        let involvement = involvement.into();
        if self.involvement == involvement {
            return Ok(());
        }
        let now = tx.now();
        tx.conn().execute_cached(
            r#"UPDATE "memberships" SET "involvement" = ?, "updated_at" = ? WHERE "memberships"."id" = ?"#,
            params![involvement, now, self.id],
        )?;
        self.involvement = involvement;
        self.updated_at = now;
        Ok(())
    }

    /// For the sidebar's badges (plans/redesign.md): how many messages from others each of
    /// someone's unread direct rooms has had since they last read it (`last_read_at`; without
    /// one, just the latest: `unread_at` is that message's time), as (room id, count).
    pub fn unread_direct_counts(conn: &Connection, user_id: i64) -> Result<Vec<(i64, i64)>> {
        crate::sql::query_all(
            conn,
            r#"SELECT "memberships"."room_id", COUNT("messages"."id") FROM "memberships"
               INNER JOIN "rooms" ON "rooms"."id" = "memberships"."room_id" AND "rooms"."type" = 'Rooms::Direct'
               INNER JOIN "messages" ON "messages"."room_id" = "memberships"."room_id" AND (CASE WHEN "memberships"."last_read_at" IS NULL THEN "messages"."created_at" >= "memberships"."unread_at" ELSE "messages"."created_at" > "memberships"."last_read_at" END)
                 AND "messages"."creator_id" != "memberships"."user_id"
               WHERE "memberships"."user_id" = ? AND "memberships"."unread_at" IS NOT NULL
               GROUP BY "memberships"."room_id""#,
            [user_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
    }

    /// `read`: `update!(unread_at: nil)`
    pub fn read(&mut self, tx: &mut Tx<'_>) -> Result<()> {
        if self.unread_at.is_none() {
            return Ok(());
        }
        let now = tx.now();
        tx.conn().execute_cached(r#"UPDATE "memberships" SET "unread_at" = ?, "updated_at" = ? WHERE "memberships"."id" = ?"#, params![None::<Timestamp>, now, self.id])?;
        Self::mark_read_at(tx, self.id, now)?;
        self.unread_at = None;
        self.updated_at = now;
        Ok(())
    }

    pub fn unread(&self) -> bool {
        self.unread_at.is_some()
    }

    /// `destroy`: the user's sockets reconnect after commit, so their subscriptions to this
    /// room are dropped.
    pub fn destroy(&self, tx: &mut Tx<'_>) -> Result<()> {
        tx.conn().execute_cached(
            r#"DELETE FROM "memberships" WHERE "memberships"."id" = ?"#,
            [self.id],
        )?;
        let user_id = self.user_id;
        tx.after_commit(move |tx| {
            User::find(tx.conn(), user_id)?.reset_remote_connections(tx);
            Ok(())
        });
        Ok(())
    }

    // The sidebar's channel order (plans/redesign.md)

    /// The rooms this person has put in order, by room id.
    pub fn positions_for(conn: &Connection, user_id: i64) -> Result<std::collections::HashMap<i64, i64>> {
        Ok(query_all(
            conn,
            r#"SELECT "room_id", "position" FROM "memberships" WHERE "user_id" = ? AND "position" IS NOT NULL"#,
            [user_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
        .into_iter()
        .collect())
    }

    /// Puts this person's rooms in the given order; rooms they aren't in are ignored.
    pub fn reorder(tx: &mut Tx<'_>, user_id: i64, room_ids: &[i64]) -> Result<()> {
        for (position, room_id) in room_ids.iter().enumerate() {
            tx.conn().execute_cached(
                r#"UPDATE "memberships" SET "position" = ? WHERE "user_id" = ? AND "room_id" = ?"#,
                params![position as i64, user_id, room_id],
            )?;
        }
        Ok(())
    }

    // Membership::Connectable

    /// `Membership.disconnect_all`
    pub fn disconnect_all(tx: &mut Tx<'_>) -> Result<usize> {
        let now = tx.now();
        Ok(tx.conn().execute_cached(
            r#"UPDATE "memberships" SET "connected_at" = ?, "connections" = ?, "updated_at" = ? WHERE "memberships"."connected_at" >= ?"#,
            params![None::<Timestamp>, 0, now, Self::connection_cutoff(now)],
        )?)
    }

    /// `Membership.connect(membership, connections)`: no `updated_at`.
    pub fn connect(tx: &mut Tx<'_>, id: i64, connections: i64) -> Result<()> {
        tx.conn().execute_cached(
            r#"UPDATE "memberships" SET "connections" = ?, "connected_at" = ?, "unread_at" = ? WHERE "memberships"."id" = ?"#,
            params![connections, tx.now(), None::<Timestamp>, id],
        )?;
        Self::mark_read_at(tx, id, tx.now())
    }

    /// This app's own `last_read_at` (schema::ADDED_COLUMNS), beside Rails' own updates. Not while
    /// the room is unread: one marked unread keeps its place until it's read (connecting clears
    /// `unread_at` first).
    fn mark_read_at(tx: &Tx<'_>, id: i64, at: Timestamp) -> Result<()> {
        tx.conn().execute_cached(r#"UPDATE "memberships" SET "last_read_at" = ? WHERE "memberships"."id" = ? AND "memberships"."unread_at" IS NULL"#, params![at, id])?;
        Ok(())
    }

    /// Slack's "Mark unread" (plans/redesign.md): unread from `message` on, for this person.
    /// `false` when they aren't in its room.
    pub fn mark_unread_from(tx: &mut Tx<'_>, user_id: i64, message: &crate::Message) -> Result<bool> {
        // A millisecond before it: the page compares milliseconds (rooms/show's #ws-last-seen).
        let before = message.created_at.ago(SignedDuration::from_millis(1));
        let changed = tx.conn().execute_cached(
            r#"UPDATE "memberships" SET "unread_at" = ?, "last_read_at" = ? WHERE "memberships"."room_id" = ? AND "memberships"."user_id" = ?"#,
            params![message.created_at, before, message.room_id, user_id],
        )?;
        Ok(changed > 0)
    }

    /// When they last had it open or read it, if this app has seen them do so.
    pub fn last_read_at(conn: &Connection, room_id: i64, user_id: i64) -> Result<Option<Timestamp>> {
        Ok(query_one(
            conn,
            r#"SELECT "last_read_at" FROM "memberships" WHERE "room_id" = ? AND "user_id" = ?"#,
            params![room_id, user_id],
            |row| row.get::<_, Option<Timestamp>>(0),
        )?
        .flatten())
    }

    /// `connected?`
    pub fn is_connected(&self, now: Timestamp) -> bool {
        self.connected_at
            .is_some_and(|at| at >= Self::connection_cutoff(now))
    }

    /// `present`
    pub fn present(&mut self, tx: &mut Tx<'_>) -> Result<()> {
        let connections = if self.is_connected(tx.now()) {
            self.connections + 1
        } else {
            1
        };
        Self::connect(tx, self.id, connections)
    }

    /// `connected`
    pub fn connected(&mut self, tx: &mut Tx<'_>) -> Result<()> {
        self.increment_connections(tx)?;
        self.touch_connected_at(tx)
    }

    /// `disconnected`
    pub fn disconnected(&mut self, tx: &mut Tx<'_>) -> Result<()> {
        self.decrement_connections(tx)?;
        if self.connections < 1 && self.connected_at.is_some() {
            let now = tx.now();
            tx.conn().execute_cached(
                r#"UPDATE "memberships" SET "connected_at" = ?, "updated_at" = ? WHERE "memberships"."id" = ?"#,
                params![None::<Timestamp>, now, self.id],
            )?;
            self.connected_at = None;
            self.updated_at = now;
        }
        Self::mark_read_at(tx, self.id, tx.now())
    }

    /// `refresh_connection`
    pub fn refresh_connection(&mut self, tx: &mut Tx<'_>) -> Result<()> {
        if !self.is_connected(tx.now()) {
            self.increment_connections(tx)?;
        }
        self.touch_connected_at(tx)
    }

    fn increment_connections(&mut self, tx: &mut Tx<'_>) -> Result<()> {
        if self.is_connected(tx.now()) {
            self.update_counter(tx, 1)
        } else {
            self.update_connections(tx, 1)
        }
    }

    fn decrement_connections(&mut self, tx: &mut Tx<'_>) -> Result<()> {
        if self.is_connected(tx.now()) {
            self.update_counter(tx, -1)
        } else {
            self.update_connections(tx, 0)
        }
    }

    /// `increment!(:connections, touch: true)` / `decrement!`
    fn update_counter(&mut self, tx: &mut Tx<'_>, by: i64) -> Result<()> {
        let now = tx.now();
        let sql = if by >= 0 {
            r#"UPDATE "memberships" SET "connections" = COALESCE("memberships"."connections", 0) + ?, "updated_at" = ? WHERE "memberships"."id" = ?"#
        } else {
            r#"UPDATE "memberships" SET "connections" = COALESCE("memberships"."connections", 0) - ?, "updated_at" = ? WHERE "memberships"."id" = ?"#
        };
        tx.conn().execute_cached(sql, params![by.abs(), now, self.id])?;
        self.connections += by;
        self.updated_at = now;
        Ok(())
    }

    /// `update!(connections:)`, a no-op when unchanged.
    fn update_connections(&mut self, tx: &mut Tx<'_>, connections: i64) -> Result<()> {
        if self.connections == connections {
            return Ok(());
        }
        let now = tx.now();
        tx.conn().execute_cached(
            r#"UPDATE "memberships" SET "connections" = ?, "updated_at" = ? WHERE "memberships"."id" = ?"#,
            params![connections, now, self.id],
        )?;
        self.connections = connections;
        self.updated_at = now;
        Ok(())
    }

    /// `touch :connected_at`
    fn touch_connected_at(&mut self, tx: &mut Tx<'_>) -> Result<()> {
        let now = tx.now();
        tx.conn().execute_cached(
            r#"UPDATE "memberships" SET "updated_at" = ?, "connected_at" = ? WHERE "memberships"."id" = ?"#,
            params![now, now, self.id],
        )?;
        Self::mark_read_at(tx, self.id, now)?;
        self.connected_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    pub fn reload(&mut self, conn: &Connection) -> Result<()> {
        *self = Self::find(conn, self.id)?;
        Ok(())
    }
}

fn room_from_prefixed_row(row: &Row<'_>) -> rusqlite::Result<Room> {
    Ok(Room {
        id: row.get("r_id")?,
        name: row.get("r_name")?,
        room_type: row.get("r_type")?,
        creator_id: row.get("r_creator_id")?,
        created_at: row.get("r_created_at")?,
        updated_at: row.get("r_updated_at")?,
        topic: row.get("r_topic").ok().flatten(),
    })
}
