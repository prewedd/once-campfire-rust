# Threads

Slack-style threads: reply to any top-level message in a thread of its own, without adding to
the room's timeline. A deliberate divergence from the Rails app (see "Known differences" in
`README.md`).

## What people see

- **Reply in thread** in every top-level message's menu. Replies to a reply go to its thread
  (threads are one level deep, as in Slack).
- Under a message with replies, a summary: the latest repliers' avatars, "N replies" and when
  the last one came in. It links to the thread.
- The thread view, `/rooms/:room_id/threads/:id`: the parent message, then its replies, with a
  composer that posts into the thread. It works like a room: live updates, paging, edits,
  boosts, deletes, uploads, @mentions, sounds, unfurls, drafts (kept per thread) and typing
  notifications (scoped to the thread).
- Replies stay out of the room's timeline, its paging and its refreshes. Search finds them, and
  their links open the thread.

## Links

- `/rooms/:room_id/threads/:id` opens a thread; `/rooms/:room_id/threads/:id/@:message_id`
  opens it at a reply, highlighted.
- A reply's permalink (`message.at_path`, "Copy link", search results) is its thread link.
  `/rooms/:room_id/@:message_id` for a reply redirects there, so older links keep working, and
  so does `/rooms/:room_id/threads/:reply_id`.

## Notifications

A reply is "news" for the thread's participants (the parent's author and everyone who has
replied) and for anyone it mentions, not for the whole room:

- **Unread**: the room goes unread (bold in the sidebar) only for them, with the usual rules:
  not for the author, not for anyone looking at the room right now, never for invisible rooms.
- **Web Push**: they're notified unless the room is muted (involvement "nothing" or
  "invisible") or they're looking at it. The title is "Thread in <room>" (or "<sender> in a
  thread" in a direct room), and tapping it opens the thread at the reply.
- Bots mentioned in a reply get the webhook as usual, with the thread in the payload, and
  their response is posted into the same thread. The bot API takes `thread_parent_id` too.

## Data

`messages.thread_parent_id` (nullable integer, indexed with `created_at`), added on boot when
missing, like the paging index. No foreign key, so the Rails image and older versions of this
one still run against the database: they ignore the column and show replies in the room.

- Creating a reply touches its parent (so the parent's cached fragment, with its summary, is
  re-rendered and room refreshes pick it up) and its room.
- Deleting a reply touches the parent. Deleting a parent deletes its replies with it.

## Realtime

Everything still goes out on the room's message stream, which only members can subscribe to.
Replies are appended to the thread's container (`thread_messages_<parent id>`), which only the
thread view has, so room views ignore them (Turbo drops actions whose target isn't on the
page). The parent's summary is replaced in place; edits, boosts and deletes already target
elements by id, so they reach whichever view shows the message.

## Out of scope for now

- "Also send to #room" when replying.
- A cross-room "Threads" inbox.
