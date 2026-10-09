# Huddles

Slack-style drop-in calls: anyone in a room (or DM) can start a huddle, others see it and join, and
it ends when the last person leaves.

## Why LiveKit

Group calls need a media server: peer-to-peer meshes fall apart past three or four people (each
laptop uploads its stream once per person) and fail behind strict NATs. LiveKit is an open-source
SFU (Apache-2.0, single Go binary) that forwards each stream without decoding it, with simulcast,
adaptive streams, an embedded TURN server and a browser client. Its server runs beside Campfire;
the client (`livekit-client` 2.22.4, ESM, vendored in `crates/assets/overrides/livekit/`) is
imported only when someone joins.

## How it fits

- **Joining**: `POST /rooms/:id/huddle` checks membership and returns a short-lived LiveKit JWT
  for room `campfire-room-<id>`, identity `user-<id>`, with the name and avatar as metadata.
  `?announce=1` (the first person in) also posts "🎧 started a huddle".
- **Who's in**: LiveKit's webhooks (`POST /livekit/webhook`, signed with the API secret over the
  body's SHA-256) update an in-memory registry; `HuddlesChannel` tells each member of the room.
  Every minute (and on boot) the registry is replaced from LiveKit's `ListRooms`/`ListParticipants`,
  as webhooks are best effort.
- **Ending**: when the last person leaves, the room's "started a huddle" message becomes
  "🎧 Huddle ended · lasted 12 min · with Ana and Ben" (`huddles::Session`). Shortcut: in memory,
  so a restart mid-huddle leaves the card at "started".
- **The client** (`lib/huddle/huddle.js`): one call per tab, kept across Turbo visits in
  `#huddle` (`data-turbo-permanent`). The bar: mute (⌘/Ctrl+Shift+Space), camera, screen share,
  reactions, raised hand, settings (devices, noise suppression, echo cancellation, automatic
  gain, a live microphone level), leave. The stage: a tile per person and per shared screen, who's
  talking, each person's volume (0–200%, Web Audio, remembered per listener). Others get a ring
  (three for a DM) and a "Join" toast; `/rooms/:id#huddle` links offer to join.

## Deployment (team.prewedd.com)

LiveKit v1.13.9 in `/root/livekit` (host networking). Signaling goes through Caddy
(`/livekit/*` → `127.0.0.1:7880`); media on 7881/tcp, 7882/udp, TURN on 3478/udp with relays on
30000–30100/udp. No TURN over TLS: Caddy owns 443, so networks that allow only 443 can't join.
