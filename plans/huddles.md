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
  talking, each person's volume (0–200%, Web Audio, remembered per listener). Also as in Slack: Add
  people (rings one person: `POST /rooms/:id/huddle/ring`, a toast and three chimes wherever they
  are), the huddle's thread (the "started a huddle" message's, opened in the side panel), and
  background blur (`@livekit/track-processors` + MediaPipe, bundled into the vendored LiveKit
  client and served from `overrides/livekit/mediapipe/`, never from a CDN). Others get a ring
  (three for a DM) and a "Join" toast; `/rooms/:id#huddle` links offer to join.

## Deployment (team.prewedd.com)

LiveKit v1.13.9 in `/root/livekit` (host networking). Signaling goes through Caddy
(`/livekit/*` → `127.0.0.1:7880`). Media: UDP 7882 and TCP 7881 straight to the server's public
IP. For networks that allow only port 443: **nginx (stream) owns 443** and routes by server name:
`turn.prewedd.com` (a DNS-only record) is TLS-terminated by nginx with Caddy's certificate for it
and passed to LiveKit's TURN on `127.0.0.2:443` (`external_tls`, PROXY protocol); every other name
passes through untouched to Caddy on `:8443`, which only accepts connections carrying nginx's
PROXY header (so it still sees client addresses). A relay-only probe connects through
`turns:turn.prewedd.com:443` (`relayProtocol=tls`). `/etc/cron.daily/nginx-reload` picks up the
certificate's renewals. Rollback: `/etc/caddy/Caddyfile.pre-sni`, `systemctl disable --now nginx`,
`/root/livekit/livekit.yaml.pre-sni`.
