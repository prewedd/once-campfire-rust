//! `reference/app/models/message.rb` and `message/*.rb` (Attachment, Mentionee, Pagination,
//! Searchable; Broadcasts belong to the app).

use rusqlite::{Connection, Row, params};

use crate::database::Tx;
use crate::error::{OptionalExt, Result};
use crate::events::Event;
use crate::models::{Attachment, Blob, Boost, RichTextRecord, Room, Sound, User};
use crate::rich_text::RichText;
use crate::sql::{self, CachedStatements, placeholders, query_all, query_one};
use crate::time::Timestamp;

/// `Message::Pagination::PAGE_SIZE`
pub const PAGE_SIZE: i64 = 40;

const RECORD_TYPE: &str = "Message";

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub id: i64,
    pub room_id: i64,
    pub creator_id: i64,
    pub client_message_id: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// The top-level message whose thread this is a reply in (plans/threads.md).
    pub thread_parent_id: Option<i64>,
}

/// Attributes for `room.messages.create!` / `create_with_attachment!`.
#[derive(Debug, Clone, Default)]
pub struct NewMessage {
    pub room_id: i64,
    pub creator_id: i64,
    /// Defaults to a random UUID (`before_create`; "Bots don't care").
    pub client_message_id: Option<String>,
    /// The stored Action Text body, if one was assigned.
    pub body: Option<String>,
    /// An already-saved blob to attach as `attachment`.
    pub attachment_blob_id: Option<i64>,
    /// Makes the message a reply in this top-level message's thread.
    pub thread_parent_id: Option<i64>,
}

/// What a top-level message's thread summary shows.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadSummary {
    pub reply_count: i64,
    pub last_reply_at: Timestamp,
    /// The latest repliers, most recent first, at most [`THREAD_SUMMARY_PARTICIPANTS`].
    pub participant_ids: Vec<i64>,
}

pub const THREAD_SUMMARY_PARTICIPANTS: i64 = 3;

/// Which messages a page comes from: a room's timeline (its top-level messages, replies being
/// in their threads) or a thread (its parent, then its replies).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timeline {
    Room(i64),
    Thread(i64),
}

impl Timeline {
    /// The timeline's messages. Its id is `?1`, so plain `?`s after it are `?2` onwards.
    fn select(self) -> &'static str {
        match self {
            Timeline::Room(_) => r#"SELECT "messages".* FROM "messages" WHERE "messages"."room_id" = ?1 AND "messages"."thread_parent_id" IS NULL"#,
            Timeline::Thread(_) => r#"SELECT "messages".* FROM "messages" WHERE ("messages"."id" = ?1 OR "messages"."thread_parent_id" = ?1)"#,
        }
    }

    fn id(self) -> i64 {
        match self {
            Timeline::Room(id) | Timeline::Thread(id) => id,
        }
    }

    /// The newest page, oldest first.
    pub fn last_page(self, conn: &Connection) -> Result<Vec<Message>> {
        let sql = format!(r#"{} ORDER BY "messages"."created_at" DESC LIMIT {PAGE_SIZE}"#, self.select());
        Ok(reversed(query_all(conn, &sql, [self.id()], Message::from_row)?))
    }

    pub fn page_before(self, conn: &Connection, message: &Message) -> Result<Vec<Message>> {
        let sql = format!(r#"{} AND (created_at < ?) ORDER BY "messages"."created_at" DESC LIMIT {PAGE_SIZE}"#, self.select());
        Ok(reversed(query_all(conn, &sql, params![self.id(), message.created_at], Message::from_row)?))
    }

    pub fn page_after(self, conn: &Connection, message: &Message) -> Result<Vec<Message>> {
        let sql = format!(r#"{} AND (created_at > ?) ORDER BY "messages"."created_at" ASC LIMIT {PAGE_SIZE}"#, self.select());
        query_all(conn, &sql, params![self.id(), message.created_at], Message::from_row)
    }

    /// Up to a page before, the message, up to a page after.
    pub fn page_around(self, conn: &Connection, message: &Message) -> Result<Vec<Message>> {
        let mut page = self.page_before(conn, message)?;
        page.push(message.clone());
        page.extend(self.page_after(conn, message)?);
        Ok(page)
    }

    pub fn page_created_since(self, conn: &Connection, time: Timestamp) -> Result<Vec<Message>> {
        let sql = format!(r#"{} AND (created_at > ?) ORDER BY "messages"."created_at" ASC LIMIT {PAGE_SIZE}"#, self.select());
        query_all(conn, &sql, params![self.id(), time], Message::from_row)
    }

    /// Messages updated since `time`, other than `excluding`.
    pub fn page_updated_since(self, conn: &Connection, time: Timestamp, excluding: &[i64]) -> Result<Vec<Message>> {
        let without = if excluding.is_empty() {
            String::new()
        } else {
            format!(r#" AND "messages"."id" NOT IN ({})"#, placeholders(excluding.len()))
        };
        let sql = format!(
            r#"{}{without} AND (updated_at > ?) ORDER BY "messages"."created_at" DESC LIMIT {PAGE_SIZE}"#,
            self.select()
        );
        let mut values: Vec<rusqlite::types::Value> = vec![self.id().into()];
        values.extend(excluding.iter().map(|id| rusqlite::types::Value::from(*id)));
        values.push(time.to_db().into());
        Ok(reversed(query_all(conn, &sql, rusqlite::params_from_iter(values), Message::from_row)?))
    }

    pub fn exists_before(self, conn: &Connection, message: &Message) -> Result<bool> {
        let sql = format!(r#"{} AND (created_at < ?) LIMIT 1"#, self.select());
        sql::exists(conn, &sql, params![self.id(), message.created_at])
    }

    pub fn exists_after(self, conn: &Connection, message: &Message) -> Result<bool> {
        let sql = format!(r#"{} AND (created_at > ?) LIMIT 1"#, self.select());
        sql::exists(conn, &sql, params![self.id(), message.created_at])
    }

    /// More than a page.
    pub fn paged(self, conn: &Connection) -> Result<bool> {
        let sql = format!(r#"{} LIMIT 1 OFFSET {PAGE_SIZE}"#, self.select());
        sql::exists(conn, &sql, [self.id()])
    }

    pub fn count(self, conn: &Connection) -> Result<i64> {
        let sql = format!(r#"SELECT COUNT(*) FROM ({})"#, self.select());
        sql::count(conn, &sql, [self.id()])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    Attachment,
    Sound,
    Text,
}

impl ContentType {
    pub fn name(self) -> &'static str {
        match self {
            ContentType::Attachment => "attachment",
            ContentType::Sound => "sound",
            ContentType::Text => "text",
        }
    }
}

const SELECT_IN_ROOM: &str =
    r#"SELECT "messages".* FROM "messages" WHERE "messages"."room_id" = ?"#;

const SELECT_REACHABLE: &str = r#"SELECT "messages".* FROM "messages" INNER JOIN "rooms" ON "messages"."room_id" = "rooms"."id" INNER JOIN "memberships" ON "rooms"."id" = "memberships"."room_id""#;

impl Message {
    pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            room_id: row.get("room_id")?,
            creator_id: row.get("creator_id")?,
            client_message_id: row.get("client_message_id")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            thread_parent_id: row.get("thread_parent_id")?,
        })
    }

    pub fn find(conn: &Connection, id: i64) -> Result<Self> {
        Self::find_by_id(conn, id)?.or_not_found("Message")
    }

    pub fn find_by_id(conn: &Connection, id: i64) -> Result<Option<Self>> {
        query_one(
            conn,
            r#"SELECT * FROM "messages" WHERE "messages"."id" = ? LIMIT 1"#,
            [id],
            Self::from_row,
        )
    }

    /// `Message.last`
    pub fn last(conn: &Connection) -> Result<Option<Self>> {
        query_one(
            conn,
            r#"SELECT * FROM "messages" ORDER BY "messages"."id" DESC LIMIT 1"#,
            [],
            Self::from_row,
        )
    }

    pub fn count(conn: &Connection) -> Result<i64> {
        sql::count(conn, r#"SELECT COUNT(*) FROM "messages""#, [])
    }

    /// `user.messages`
    pub fn by_creator(conn: &Connection, creator_id: i64) -> Result<Vec<Self>> {
        query_all(
            conn,
            r#"SELECT "messages".* FROM "messages" WHERE "messages"."creator_id" = ?"#,
            [creator_id],
            Self::from_row,
        )
    }

    /// `room.messages`
    pub fn for_room(conn: &Connection, room_id: i64) -> Result<Vec<Self>> {
        query_all(conn, SELECT_IN_ROOM, [room_id], Self::from_row)
    }

    /// `room.messages.find(id)`
    pub fn find_in_room(conn: &Connection, room_id: i64, id: i64) -> Result<Self> {
        let sql = format!(r#"{SELECT_IN_ROOM} AND "messages"."id" = ? LIMIT 1"#);
        query_one(conn, &sql, [room_id, id], Self::from_row)?.or_not_found("Message")
    }

    /// `room.messages.count`: the room's timeline, not counting replies.
    pub fn count_in_room(conn: &Connection, room_id: i64) -> Result<i64> {
        Timeline::Room(room_id).count(conn)
    }

    /// `Current.user.reachable_messages.find(id)`
    pub fn find_reachable(conn: &Connection, user_id: i64, id: i64) -> Result<Self> {
        let sql = format!(
            r#"{SELECT_REACHABLE} WHERE "memberships"."user_id" = ? AND "messages"."id" = ? LIMIT 1"#
        );
        query_one(conn, &sql, [user_id, id], Self::from_row)?.or_not_found("Message")
    }

    // Message::Pagination, over the room's timeline: replies are paged in their threads.

    /// `room.messages.last_page`: the newest 40, oldest first.
    pub fn last_page(conn: &Connection, room_id: i64) -> Result<Vec<Self>> {
        Timeline::Room(room_id).last_page(conn)
    }

    /// `room.messages.first_page`
    pub fn first_page(conn: &Connection, room_id: i64) -> Result<Vec<Self>> {
        let sql = format!(r#"{} ORDER BY "messages"."created_at" ASC LIMIT {PAGE_SIZE}"#, Timeline::Room(room_id).select());
        query_all(conn, &sql, [room_id], Self::from_row)
    }

    /// `room.messages.page_before(message)`
    pub fn page_before(conn: &Connection, room_id: i64, message: &Message) -> Result<Vec<Self>> {
        Timeline::Room(room_id).page_before(conn, message)
    }

    /// `room.messages.page_after(message)`
    pub fn page_after(conn: &Connection, room_id: i64, message: &Message) -> Result<Vec<Self>> {
        Timeline::Room(room_id).page_after(conn, message)
    }

    /// `room.messages.page_around(message)`: up to 40 before, the message, up to 40 after.
    pub fn page_around(conn: &Connection, room_id: i64, message: &Message) -> Result<Vec<Self>> {
        Timeline::Room(room_id).page_around(conn, message)
    }

    /// `room.messages.page_created_since(time)`
    pub fn page_created_since(conn: &Connection, room_id: i64, time: Timestamp) -> Result<Vec<Self>> {
        Timeline::Room(room_id).page_created_since(conn, time)
    }

    /// `room.messages.without(excluding).page_updated_since(time)`
    pub fn page_updated_since(conn: &Connection, room_id: i64, time: Timestamp, excluding: &[i64]) -> Result<Vec<Self>> {
        Timeline::Room(room_id).page_updated_since(conn, time, excluding)
    }

    /// `room.messages.before(message).exists?`
    pub fn exists_before(conn: &Connection, room_id: i64, message: &Message) -> Result<bool> {
        Timeline::Room(room_id).exists_before(conn, message)
    }

    /// `room.messages.after(message).exists?`
    pub fn exists_after(conn: &Connection, room_id: i64, message: &Message) -> Result<bool> {
        Timeline::Room(room_id).exists_after(conn, message)
    }

    /// `room.messages.paged?`: more than a page (`count > PAGE_SIZE`). Asks whether a row exists
    /// past the first page rather than counting the whole room, which grows without bound.
    pub fn paged(conn: &Connection, room_id: i64) -> Result<bool> {
        Timeline::Room(room_id).paged(conn)
    }

    // Threads (plans/threads.md)

    /// The message that heads this one's thread: its parent, or itself.
    pub fn thread_root_id(&self) -> i64 {
        self.thread_parent_id.unwrap_or(self.id)
    }

    pub fn is_reply(&self) -> bool {
        self.thread_parent_id.is_some()
    }

    /// A thread's replies, oldest first.
    pub fn replies(conn: &Connection, parent_id: i64) -> Result<Vec<Self>> {
        query_all(
            conn,
            r#"SELECT "messages".* FROM "messages" WHERE "messages"."thread_parent_id" = ? ORDER BY "messages"."created_at" ASC"#,
            [parent_id],
            Self::from_row,
        )
    }

    /// A top-level message's summary, or `None` while it has no replies.
/// The threads someone is in (started, or replied to), in rooms they're still in, the most
    /// recently active first: each parent's id and its last reply's time.
    pub fn threads_for(conn: &Connection, user_id: i64, limit: i64) -> Result<Vec<(i64, Timestamp)>> {
        query_all(
            conn,
            r#"SELECT "parents"."id", MAX("replies"."created_at") AS "last_reply_at"
               FROM "messages" AS "parents"
               INNER JOIN "messages" AS "replies" ON "replies"."thread_parent_id" = "parents"."id"
               INNER JOIN "memberships" ON "memberships"."room_id" = "parents"."room_id" AND "memberships"."user_id" = ?1
               WHERE "parents"."creator_id" = ?1
                  OR EXISTS (SELECT 1 FROM "messages" AS "mine" WHERE "mine"."thread_parent_id" = "parents"."id" AND "mine"."creator_id" = ?1)
               GROUP BY "parents"."id"
               ORDER BY "last_reply_at" DESC
               LIMIT ?2"#,
            params![user_id, limit],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
    }

        pub fn thread_summary(conn: &Connection, parent_id: i64) -> Result<Option<ThreadSummary>> {
        let counted: (i64, Option<Timestamp>) = conn.query_row_cached(
            r#"SELECT COUNT(*), MAX("messages"."created_at") FROM "messages" WHERE "messages"."thread_parent_id" = ?"#,
            [parent_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let (reply_count, Some(last_reply_at)) = counted else { return Ok(None) };
        if reply_count == 0 {
            return Ok(None);
        }
        let participant_ids = query_all(
            conn,
            &format!(
                r#"SELECT "messages"."creator_id" FROM "messages" WHERE "messages"."thread_parent_id" = ? GROUP BY "messages"."creator_id" ORDER BY MAX("messages"."created_at") DESC LIMIT {THREAD_SUMMARY_PARTICIPANTS}"#
            ),
            [parent_id],
            |r| r.get(0),
        )?;
        Ok(Some(ThreadSummary { reply_count, last_reply_at, participant_ids }))
    }

    /// Everyone who has posted in a thread: the parent's author, then the repliers.
    pub fn thread_participant_ids(conn: &Connection, parent_id: i64) -> Result<Vec<i64>> {
        query_all(
            conn,
            r#"SELECT "messages"."creator_id" FROM "messages" WHERE "messages"."id" = ?1 UNION SELECT "messages"."creator_id" FROM "messages" WHERE "messages"."thread_parent_id" = ?1"#,
            [parent_id],
            |r| r.get(0),
        )
    }

    /// Who a reply is news for: its thread's participants and the room members it mentions,
    /// other than its author. Empty for a top-level message.
    pub fn thread_audience_ids(&self, conn: &Connection, rich_text: &dyn RichText) -> Result<Vec<i64>> {
        let Some(parent_id) = self.thread_parent_id else { return Ok(Vec::new()) };
        let mut ids = Self::thread_participant_ids(conn, parent_id)?;
        ids.extend(self.mentionees(conn, rich_text)?.into_iter().map(|user| user.id));
        ids.sort_unstable();
        ids.dedup();
        ids.retain(|id| *id != self.creator_id);
        Ok(ids)
    }

    // Message::Searchable

    /// `room.messages.search(query)`: FTS5 `MATCH`, in message order.
    pub fn search_in_room(conn: &Connection, room_id: i64, query: &str) -> Result<Vec<Self>> {
        let query = match_terms(query);
        if query.is_empty() {
            return Ok(Vec::new());
        }
        query_all(
            conn,
            r#"SELECT "messages".* FROM "messages" join message_search_index idx on messages.id = idx.rowid WHERE "messages"."room_id" = ? AND (idx.body match ?) ORDER BY "messages"."created_at" ASC"#,
            params![room_id, query],
            Self::from_row,
        )
    }

    /// `Current.user.reachable_messages.search(query).last(100)`
    pub fn search_reachable(conn: &Connection, user_id: i64, query: &str) -> Result<Vec<Self>> {
        let query = match_terms(query);
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            r#"{SELECT_REACHABLE} join message_search_index idx on messages.id = idx.rowid WHERE "memberships"."user_id" = ? AND (idx.body match ?) ORDER BY "messages"."created_at" DESC LIMIT 100"#
        );
        Ok(reversed(query_all(
            conn,
            &sql,
            params![user_id, query],
            Self::from_row,
        )?))
    }

    // Creating, updating, destroying

    /// `room.messages.create!` (or `create_with_attachment!` given a blob). Inside the
    /// transaction: the message, its body (which touches the message), its attachment, and
    /// the room touch. After commit: the search index, then `room.receive` (unread
    /// memberships and the push job).
    pub fn create(tx: &mut Tx<'_>, attributes: NewMessage) -> Result<Self> {
        let now = tx.now();
        let client_message_id = attributes.client_message_id.unwrap_or_else(sql::uuid);
        let id: i64 = match attributes.thread_parent_id {
            // Rails' own insert, so a top-level message is written exactly as before.
            None => tx.conn().query_row_cached(
                r#"INSERT INTO "messages" ("client_message_id", "created_at", "creator_id", "room_id", "updated_at") VALUES (?, ?, ?, ?, ?) RETURNING "id""#,
                params![client_message_id, now, attributes.creator_id, attributes.room_id, now],
                |r| r.get(0),
            )?,
            Some(parent_id) => tx.conn().query_row_cached(
                r#"INSERT INTO "messages" ("client_message_id", "created_at", "creator_id", "room_id", "updated_at", "thread_parent_id") VALUES (?, ?, ?, ?, ?, ?) RETURNING "id""#,
                params![client_message_id, now, attributes.creator_id, attributes.room_id, now, parent_id],
                |r| r.get(0),
            )?,
        };
        let mut message = Self {
            id,
            room_id: attributes.room_id,
            creator_id: attributes.creator_id,
            client_message_id,
            created_at: now,
            updated_at: now,
            thread_parent_id: attributes.thread_parent_id,
        };

        let mut touched = false;
        if let Some(body) = &attributes.body {
            RichTextRecord::create(tx, RECORD_TYPE, id, "body", body)?;
            touched = true;
        }
        if let Some(blob_id) = attributes.attachment_blob_id {
            Attachment::create(tx, RECORD_TYPE, id, "attachment", blob_id)?;
            touched = true;
        }
        if touched {
            message.updated_at = Self::touch_row(tx, id)?;
        }
        // The parent's summary changes with it.
        if let Some(parent_id) = message.thread_parent_id {
            Self::touch_row(tx, parent_id)?;
        }
        Room::touch(tx, message.room_id)?;

        let committed = message.clone();
        tx.after_commit(move |tx| {
            committed.create_in_index(tx)?;
            match committed.thread_parent_id {
                None => Room::receive(tx, committed.room_id, &committed),
                Some(_) => Room::receive_reply(tx, committed.room_id, &committed),
            }
        });
        Ok(message)
    }

    /// `message.update!(body:)`: the body changes, which touches the message and its room;
    /// the search index follows after commit.
    pub fn update_body(&mut self, tx: &mut Tx<'_>, body: &str) -> Result<()> {
        match RichTextRecord::find_for(tx.conn(), RECORD_TYPE, self.id, "body")? {
            Some(record) if record.body.as_deref() == Some(body) => return Ok(()),
            Some(mut record) => record.update_body(tx, body)?,
            None => {
                RichTextRecord::create(tx, RECORD_TYPE, self.id, "body", body)?;
            }
        }
        self.touch(tx)
    }

    /// `touch` (from a boost, or the body): the message, then its room, then a reindex
    /// after commit.
    pub fn touch(&mut self, tx: &mut Tx<'_>) -> Result<()> {
        self.updated_at = Self::touch_row(tx, self.id)?;
        Room::touch(tx, self.room_id)?;
        let id = self.id;
        tx.after_commit(move |tx| Message::find(tx.conn(), id)?.update_in_index(tx));
        Ok(())
    }

    fn touch_row(tx: &Tx<'_>, id: i64) -> Result<Timestamp> {
        let now = tx.now();
        tx.conn().execute_cached(
            r#"UPDATE "messages" SET "updated_at" = ? WHERE "messages"."id" = ?"#,
            params![now, id],
        )?;
        Ok(now)
    }

    /// `update!(attachment: blob or nil)`: `has_one_attached` replaces the attachment. The old
    /// one is destroyed (its blob purged after commit, `dependent: :purge_later`) and each
    /// attachment change touches the message (`belongs_to :record, touch: true`), and so its room.
    pub fn replace_attachment(&mut self, tx: &mut Tx<'_>, blob_id: Option<i64>) -> Result<()> {
        if let Some(attachment) =
            Attachment::find_for(tx.conn(), RECORD_TYPE, self.id, "attachment")?
        {
            attachment.delete(tx)?;
            tx.emit_after_commit(Event::PurgeBlob {
                blob_id: attachment.blob_id,
            });
            self.touch(tx)?;
        }
        if let Some(blob_id) = blob_id {
            Attachment::create(tx, RECORD_TYPE, self.id, "attachment", blob_id)?;
            self.touch(tx)?;
        }
        Ok(())
    }

    /// `destroy`: its attachment (blob purged later), boosts and body, then the message, and
    /// the room is touched. The search index entry goes after commit. A thread goes with its
    /// parent; a reply's parent is touched, as its summary changes.
    pub fn destroy(&self, tx: &mut Tx<'_>) -> Result<()> {
        match self.thread_parent_id {
            None => {
                for reply in Self::replies(tx.conn(), self.id)? {
                    reply.destroy_row(tx)?;
                }
            }
            Some(parent_id) => {
                Self::touch_row(tx, parent_id)?;
            }
        }
        self.destroy_row(tx)
    }

    fn destroy_row(&self, tx: &mut Tx<'_>) -> Result<()> {
        if let Some(attachment) =
            Attachment::find_for(tx.conn(), RECORD_TYPE, self.id, "attachment")?
        {
            attachment.delete(tx)?;
            tx.emit_after_commit(Event::PurgeBlob {
                blob_id: attachment.blob_id,
            });
        }
        for boost in Boost::for_message(tx.conn(), self.id)? {
            boost.delete_row(tx)?;
        }
        if let Some(body) = RichTextRecord::find_for(tx.conn(), RECORD_TYPE, self.id, "body")? {
            body.delete(tx)?;
        }
        tx.conn().execute_cached(
            r#"DELETE FROM "messages" WHERE "messages"."id" = ?"#,
            [self.id],
        )?;
        Room::touch(tx, self.room_id)?;
        let id = self.id;
        tx.after_commit(move |tx| remove_from_index(tx, id));
        Ok(())
    }

    fn create_in_index(&self, tx: &Tx<'_>) -> Result<()> {
        let body = self.plain_text_body(tx.conn(), tx.rich_text())?;
        tx.conn().execute_cached(
            "insert into message_search_index(rowid, body) values (?, ?)",
            params![self.id, body],
        )?;
        Ok(())
    }

    fn update_in_index(&self, tx: &Tx<'_>) -> Result<()> {
        let body = self.plain_text_body(tx.conn(), tx.rich_text())?;
        tx.conn().execute_cached(
            "update message_search_index set body = ? where rowid = ?",
            params![body, self.id],
        )?;
        Ok(())
    }

    // Attributes and associations

    pub fn room(&self, conn: &Connection) -> Result<Room> {
        Room::find(conn, self.room_id)
    }

    pub fn creator(&self, conn: &Connection) -> Result<User> {
        User::find(conn, self.creator_id)
    }

    pub fn boosts(&self, conn: &Connection) -> Result<Vec<Boost>> {
        Boost::for_message_ordered(conn, self.id)
    }

    pub fn body(&self, conn: &Connection) -> Result<Option<RichTextRecord>> {
        RichTextRecord::find_for(conn, RECORD_TYPE, self.id, "body")
    }

    /// `message.body.body` as stored HTML, if any.
    pub fn body_html(&self, conn: &Connection) -> Result<Option<String>> {
        Ok(self.body(conn)?.and_then(|b| b.body))
    }

    /// The attachment and its blob, if attached.
    pub fn attachment(&self, conn: &Connection) -> Result<Option<(Attachment, Blob)>> {
        match Attachment::find_for(conn, RECORD_TYPE, self.id, "attachment")? {
            Some(attachment) => {
                let blob = attachment.blob(conn)?;
                Ok(Some((attachment, blob)))
            }
            None => Ok(None),
        }
    }

    /// `plain_text_body`: `body.to_plain_text.presence || attachment&.filename&.to_s || ""`
    pub fn plain_text_body(&self, conn: &Connection, rich_text: &dyn RichText) -> Result<String> {
        if let Some(html) = self.body_html(conn)? {
            let text = rich_text.to_plain_text(conn, &html, &|id| {
                User::find_by_id(conn, id).ok().flatten().map(|u| u.name)
            });
            if !text.trim().is_empty() {
                return Ok(text);
            }
        }
        Ok(self
            .attachment(conn)?
            .map(|(_, blob)| blob.filename)
            .unwrap_or_default())
    }

    /// `content_type`
    pub fn content_type(&self, conn: &Connection, rich_text: &dyn RichText) -> Result<ContentType> {
        if self.attachment(conn)?.is_some() {
            Ok(ContentType::Attachment)
        } else if self.sound(conn, rich_text)?.is_some() {
            Ok(ContentType::Sound)
        } else {
            Ok(ContentType::Text)
        }
    }

    /// `sound`: a body of exactly `/play <name>` naming a built-in sound.
    pub fn sound(
        &self,
        conn: &Connection,
        rich_text: &dyn RichText,
    ) -> Result<Option<&'static Sound>> {
        Ok(sound_in(&self.plain_text_body(conn, rich_text)?))
    }

    /// `mentionees`: mentioned users who are members of the room.
    pub fn mentionees(&self, conn: &Connection, rich_text: &dyn RichText) -> Result<Vec<User>> {
        let ids = match self.body_html(conn)? {
            Some(html) => rich_text.mentioned_user_ids(conn, &html),
            None => Vec::new(),
        };
        mentionees_in_room(conn, self.room_id, &ids)
    }

    pub fn reload(&mut self, conn: &Connection) -> Result<()> {
        *self = Self::find(conn, self.id)?;
        Ok(())
    }
}

/// `room.users.where(id: ids)`
pub fn mentionees_in_room(conn: &Connection, room_id: i64, user_ids: &[i64]) -> Result<Vec<User>> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        r#"SELECT "users".* FROM "users" INNER JOIN "memberships" ON "users"."id" = "memberships"."user_id" WHERE "memberships"."room_id" = ? AND "users"."id" IN ({})"#,
        placeholders(user_ids.len())
    );
    let values: Vec<i64> = std::iter::once(room_id)
        .chain(user_ids.iter().copied())
        .collect();
    query_all(
        conn,
        &sql,
        rusqlite::params_from_iter(values),
        User::from_row,
    )
}

/// `plain_text_body.match(/\A\/play (?<name>\w+)\z/)` then `Sound.find_by_name`.
pub fn sound_in(plain_text: &str) -> Option<&'static Sound> {
    let name = plain_text.strip_prefix("/play ")?;
    // `\z` allows no trailing newline; `\w` is ASCII word characters.
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    Sound::find_by_name(name)
}

fn remove_from_index(tx: &Tx<'_>, id: i64) -> Result<()> {
    tx.conn()
        .execute_cached("delete from message_search_index where rowid = ?", [id])?;
    Ok(())
}

fn reversed<T>(mut rows: Vec<T>) -> Vec<T> {
    rows.reverse();
    rows
}

/// Each word of a search as an FTS5 string, so every word must appear and none is read as query
/// syntax. Rails passes the words straight to `MATCH`, where `NOT`, `AND`, `OR` or `NEAR` in the
/// wrong place is a syntax error (a 500).
/// NULs separate words too: SQLite would end the query string at one.
fn match_terms(query: &str) -> String {
    query
        .split(|c: char| c.is_whitespace() || c == '\0')
        .filter(|word| !word.is_empty())
        .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}
