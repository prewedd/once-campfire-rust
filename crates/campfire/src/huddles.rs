//! Huddles (plans/huddles.md): drop-in voice and video calls in a room, as in Slack, with LiveKit
//! as the media server. Campfire signs who may join which room's call, and keeps track of who's in
//! each one from LiveKit's webhooks (reconciled with LiveKit's own list on boot and every minute,
//! as webhooks are best effort), so everyone in the room can see the huddle and join it.
//!
//! Off unless LIVEKIT_URL (what browsers connect to), LIVEKIT_API_KEY and LIVEKIT_API_SECRET are
//! set. LIVEKIT_INTERNAL_URL is where Campfire reaches LiveKit's API (http://127.0.0.1:7880).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// How long a join token is good for: the call itself can last longer.
const TOKEN_TTL_SECONDS: i64 = 6 * 60 * 60;
/// LiveKit's own clock leeway for tokens.
const LEEWAY_SECONDS: i64 = 60;

#[derive(Debug, Clone)]
pub struct Settings {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
    pub internal_url: String,
}

pub static SETTINGS: LazyLock<Option<Settings>> = LazyLock::new(|| {
    let var = |name: &str| std::env::var(name).ok().filter(|value| !value.trim().is_empty());
    Some(Settings {
        url: var("LIVEKIT_URL")?,
        api_key: var("LIVEKIT_API_KEY")?,
        api_secret: var("LIVEKIT_API_SECRET")?,
        internal_url: var("LIVEKIT_INTERNAL_URL").unwrap_or_else(|| "http://127.0.0.1:7880".into()),
    })
});

pub fn enabled() -> bool {
    SETTINGS.is_some()
}

/// The LiveKit room for a Campfire room's huddle.
pub fn livekit_room(room_id: i64) -> String {
    format!("campfire-room-{room_id}")
}

fn room_id_of(livekit_room: &str) -> Option<i64> {
    livekit_room.strip_prefix("campfire-room-")?.parse().ok()
}

fn identity(user_id: i64) -> String {
    format!("user-{user_id}")
}

fn user_id_of(identity: &str) -> Option<i64> {
    identity.strip_prefix("user-")?.parse().ok()
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

// --- Tokens (HS256 JWTs, as LiveKit's protocol/auth makes them) ------------------------------------

fn sign(secret: &str, claims: &Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
    let signing_input = format!("{header}.{payload}");
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC takes any key");
    mac.update(signing_input.as_bytes());
    format!("{signing_input}.{}", URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

/// What a browser needs to join a room's huddle as `user`.
pub fn join_token(settings: &Settings, user_id: i64, name: &str, avatar_url: &str, room_id: i64, now: i64) -> String {
    sign(
        &settings.api_secret,
        &json!({
            "iss": settings.api_key,
            "sub": identity(user_id),
            "name": name,
            "metadata": json!({ "user_id": user_id, "avatar_url": avatar_url }).to_string(),
            "nbf": now - LEEWAY_SECONDS,
            "iat": now,
            "exp": now + TOKEN_TTL_SECONDS,
            "video": {
                "roomJoin": true,
                "room": livekit_room(room_id),
                "canPublish": true,
                "canSubscribe": true,
                "canPublishData": true,
                "canUpdateOwnMetadata": true,
            },
        }),
    )
}

/// For Campfire's own calls to LiveKit's RoomService: `roomList`, and `roomAdmin` of one room.
fn service_token(settings: &Settings, room: Option<&str>, now: i64) -> String {
    let video = match room {
        Some(room) => json!({ "roomAdmin": true, "room": room }),
        None => json!({ "roomList": true }),
    };
    sign(&settings.api_secret, &json!({ "iss": settings.api_key, "nbf": now - LEEWAY_SECONDS, "exp": now + 60, "video": video }))
}

/// The claims of a token this deployment's secret signed, if it did and it's current.
fn verify(settings: &Settings, token: &str, now: i64) -> Option<Value> {
    let (signing_input, signature) = token.rsplit_once('.')?;
    let (header, payload) = signing_input.split_once('.')?;
    let header: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header).ok()?).ok()?;
    if header["alg"] != "HS256" {
        return None;
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(settings.api_secret.as_bytes()).ok()?;
    mac.update(signing_input.as_bytes());
    mac.verify_slice(&URL_SAFE_NO_PAD.decode(signature).ok()?).ok()?;
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()?;
    let within = claims["nbf"].as_i64().is_none_or(|nbf| nbf <= now + LEEWAY_SECONDS)
        && claims["exp"].as_i64().is_some_and(|exp| now <= exp + LEEWAY_SECONDS)
        && claims["iss"] == settings.api_key.as_str();
    within.then_some(claims)
}

/// A webhook from LiveKit: its `Authorization` header is a token over the SHA-256 of the body.
pub fn verify_webhook(settings: &Settings, authorization: &str, body: &[u8], now: i64) -> Option<Value> {
    let claims = verify(settings, authorization.trim().trim_start_matches("Bearer "), now)?;
    let expected = STANDARD.encode(Sha256::digest(body));
    let given = claims["sha256"].as_str()?;
    (expected.len() == given.len() && expected.bytes().zip(given.bytes()).fold(0, |diff, (a, b)| diff | (a ^ b)) == 0).then_some(())?;
    serde_json::from_slice(body).ok()
}

// --- Who's in which huddle --------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Participant {
    pub user_id: i64,
    pub name: String,
    pub avatar_url: String,
}

/// Room id → the people in its huddle, in the order they joined.
static HUDDLES: LazyLock<Mutex<HashMap<i64, Vec<Participant>>>> = LazyLock::new(Mutex::default);

pub fn participants(room_id: i64) -> Vec<Participant> {
    HUDDLES.lock().unwrap().get(&room_id).cloned().unwrap_or_default()
}

/// Every huddle in progress.
pub fn all() -> HashMap<i64, Vec<Participant>> {
    HUDDLES.lock().unwrap().clone()
}

fn participant_from(value: &Value) -> Option<Participant> {
    let user_id = user_id_of(value["identity"].as_str()?)?;
    let metadata: Value = value["metadata"].as_str().and_then(|m| serde_json::from_str(m).ok()).unwrap_or_default();
    Some(Participant {
        user_id,
        name: value["name"].as_str().unwrap_or_default().to_string(),
        avatar_url: metadata["avatar_url"].as_str().unwrap_or_default().to_string(),
    })
}

/// Applies a webhook event; the room whose huddle changed, if one did.
pub fn apply(event: &Value) -> Option<i64> {
    let room_id = room_id_of(event["room"]["name"].as_str()?)?;
    let mut huddles = HUDDLES.lock().unwrap();
    let before = huddles.get(&room_id).cloned().unwrap_or_default();
    match event["event"].as_str()? {
        "participant_joined" => {
            let participant = participant_from(&event["participant"])?;
            let people = huddles.entry(room_id).or_default();
            if !people.iter().any(|p| p.user_id == participant.user_id) {
                people.push(participant);
            }
        }
        "participant_left" | "participant_connection_aborted" => {
            let user_id = user_id_of(event["participant"]["identity"].as_str()?)?;
            if let Some(people) = huddles.get_mut(&room_id) {
                people.retain(|p| p.user_id != user_id);
                if people.is_empty() {
                    huddles.remove(&room_id);
                }
            }
        }
        "room_finished" => {
            huddles.remove(&room_id);
        }
        _ => return None,
    }
    let after = huddles.get(&room_id).cloned().unwrap_or_default();
    (before != after).then_some(room_id)
}

/// Replaces what's known with LiveKit's own list; the rooms whose huddles changed.
pub fn replace(current: HashMap<i64, Vec<Participant>>) -> Vec<i64> {
    let mut huddles = HUDDLES.lock().unwrap();
    let mut changed: Vec<i64> = huddles.keys().chain(current.keys()).copied().filter(|id| huddles.get(id) != current.get(id)).collect();
    changed.sort_unstable();
    changed.dedup();
    *huddles = current;
    changed
}

// --- LiveKit's RoomService (Twirp, JSON) ------------------------------------------------------------

async fn room_service(settings: &Settings, method: &str, room: Option<&str>, body: Value) -> anyhow::Result<Value> {
    use http_body_util::{BodyExt, Full};
    use hyper_util::client::legacy::Client;
    use hyper_util::rt::TokioExecutor;

    let uri = format!("{}/twirp/livekit.RoomService/{method}", settings.internal_url.trim_end_matches('/'));
    let request = hyper::Request::post(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", service_token(settings, room, now())))
        .body(Full::new(hyper::body::Bytes::from(body.to_string())))?;
    let client = Client::builder(TokioExecutor::new()).build_http();
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), client.request(request)).await??;
    let status = response.status();
    let bytes = response.into_body().collect().await?.to_bytes();
    anyhow::ensure!(status.is_success(), "LiveKit {method}: {status}");
    Ok(serde_json::from_slice(&bytes)?)
}

/// Who's in every Campfire huddle, as LiveKit has it.
pub async fn fetch(settings: &Settings) -> anyhow::Result<HashMap<i64, Vec<Participant>>> {
    let rooms = room_service(settings, "ListRooms", None, json!({})).await?;
    let mut huddles = HashMap::new();
    for room in rooms["rooms"].as_array().into_iter().flatten() {
        let Some(name) = room["name"].as_str() else { continue };
        let Some(room_id) = room_id_of(name) else { continue };
        let listed = room_service(settings, "ListParticipants", Some(name), json!({ "room": name })).await?;
        let mut people: Vec<(i64, Participant)> = listed["participants"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| Some((p["joinedAt"].as_str().and_then(|t| t.parse().ok()).unwrap_or(0), participant_from(p)?)))
            .collect();
        people.sort_by_key(|(joined_at, _)| *joined_at);
        if !people.is_empty() {
            huddles.insert(room_id, people.into_iter().map(|(_, p)| p).collect());
        }
    }
    Ok(huddles)
}

pub fn unix_now() -> i64 {
    now()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings { url: "wss://chat.test/livekit".into(), api_key: "APIkey".into(), api_secret: "secret-secret-secret".into(), internal_url: "http://127.0.0.1:7880".into() }
    }

    fn claims_of(token: &str) -> Value {
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(token.split('.').nth(1).unwrap()).unwrap()).unwrap()
    }

    #[test]
    fn join_tokens_let_one_person_into_one_rooms_huddle() {
        let token = join_token(&settings(), 42, "Alice", "/u/42.png", 7, 1_760_000_000);
        let claims = claims_of(&token);
        assert_eq!(claims["iss"], "APIkey");
        assert_eq!(claims["sub"], "user-42");
        assert_eq!(claims["name"], "Alice");
        assert_eq!(claims["video"]["room"], "campfire-room-7");
        assert_eq!(claims["video"]["roomJoin"], true);
        assert!(claims["video"].get("roomAdmin").is_none());
        assert_eq!(claims["exp"].as_i64().unwrap() - claims["iat"].as_i64().unwrap(), TOKEN_TTL_SECONDS);
        assert!(verify(&settings(), &token, 1_760_000_000).is_some(), "signed with the secret");
        let other = Settings { api_secret: "another-secret".into(), ..settings() };
        assert!(verify(&other, &token, 1_760_000_000).is_none());
        assert!(verify(&settings(), &token, 1_760_000_000 + TOKEN_TTL_SECONDS + 120).is_none(), "expired");
    }

    #[test]
    fn webhooks_must_be_signed_over_their_body() {
        let body = br#"{"event":"participant_joined","room":{"name":"campfire-room-7"},"participant":{"identity":"user-42","name":"Alice","metadata":"{\"avatar_url\":\"/u/42.png\"}"}}"#;
        let now = 1_760_000_000;
        let auth = sign("secret-secret-secret", &json!({ "iss": "APIkey", "nbf": now, "exp": now + 300, "sha256": STANDARD.encode(Sha256::digest(body)) }));
        assert!(verify_webhook(&settings(), &auth, body, now).is_some());
        assert!(verify_webhook(&settings(), &auth, b"{\"event\":\"room_finished\"}", now).is_none(), "another body");
        assert!(verify_webhook(&settings(), "not a token", body, now).is_none());
    }

    #[test]
    fn events_track_who_is_in_a_huddle() {
        let joined = |id: i64, name: &str| json!({ "event": "participant_joined", "room": { "name": "campfire-room-99" }, "participant": { "identity": format!("user-{id}"), "name": name, "metadata": "{\"avatar_url\":\"/a.png\"}" } });
        let left = |id: i64| json!({ "event": "participant_left", "room": { "name": "campfire-room-99" }, "participant": { "identity": format!("user-{id}") } });
        assert_eq!(apply(&joined(1, "Ana")), Some(99));
        assert_eq!(apply(&joined(2, "Ben")), Some(99));
        assert_eq!(apply(&joined(2, "Ben")), None, "already there");
        assert_eq!(participants(99).iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Ana", "Ben"]);
        assert_eq!(participants(99)[0].avatar_url, "/a.png");
        assert_eq!(apply(&left(1)), Some(99));
        assert_eq!(apply(&json!({ "event": "room_finished", "room": { "name": "campfire-room-99" } })), Some(99));
        assert!(participants(99).is_empty());
        assert_eq!(apply(&json!({ "event": "track_published", "room": { "name": "campfire-room-99" } })), None);
        assert_eq!(apply(&json!({ "event": "participant_joined", "room": { "name": "someone-elses" } })), None);
    }
}
