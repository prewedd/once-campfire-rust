# Scheduled messages

Write now, post later, as in Slack: the composer's clock offers "In 30 minutes", "In 1 hour",
"Tomorrow" (9 AM), "Next Monday" (9 AM) or any date and time, in your own time zone.

## Reliability

- A scheduled message is a row in `scheduled_messages` (created on boot, with an index on
  `send_at` for those not yet posted). Its body is canonicalized when it's scheduled, as a message's
  is when posted.
- A background task checks every 5 seconds for those due. Posting one creates the message and sets
  the row's `message_id` in the same transaction, so each is posted exactly once: a crash before
  the commit posts nothing (it's tried again), and after it the row is marked.
- Anything that came due while the app was down goes out when it's back (late rather than lost).
- Posting checks the writer is still an active member of the room; if not, the row is dropped. A
  reply whose thread went in the meantime goes to the room.
- After the commit the message is broadcast, pushed and sent to bots like any other.

## API (JSON, for `lib/workspace/schedule.js`)

`GET /rooms/:id/scheduled_messages` (`?all=1`: every room), `POST` (`body`, `send_at` in Unix
seconds, `thread_parent_id`), `PATCH …/:id` (`send_at`), `DELETE …/:id`, `POST …/:id/deliver`
(send now). Times must be at least 30 seconds and at most 120 days ahead; a 422 says why not.

Shortcut: no attachments yet (the menu says so).
