//! `HuddlesChannel`: who's in each huddle of the rooms you're in (plans/huddles.md). A new
//! subscriber gets them all; changes arrive as `{ room_id, participants }`.
use std::collections::HashMap;

use campfire_cable::{Channel, ChannelResult, Params, Subscription};
use campfire_db::{Database, Membership};
use serde::Serialize;

use super::CableUser;
use crate::huddles::{self, Participant};

/// Per user, so a huddle only reaches the room's members.
pub fn stream_name_for(user_id: i64) -> String {
    format!("user_{user_id}_huddles")
}

#[derive(Serialize)]
pub struct Change<'a> {
    pub room_id: i64,
    pub participants: &'a [Participant],
}

#[derive(Serialize)]
struct Snapshot {
    huddles: HashMap<i64, Vec<Participant>>,
}

pub struct HuddlesChannel {
    db: Database,
}

impl HuddlesChannel {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl Channel<CableUser> for HuddlesChannel {
    async fn subscribed(&mut self, sub: &mut Subscription<CableUser>) -> ChannelResult {
        let user_id = sub.current_user().id;
        sub.stream_from(stream_name_for(user_id));
        let rooms: Vec<i64> = self.db.read(move |conn| Membership::for_user(conn, user_id)).await?.into_iter().map(|m| m.room_id).collect();
        let huddles = huddles::all().into_iter().filter(|(room_id, _)| rooms.contains(room_id)).collect();
        sub.transmit(&Snapshot { huddles });
        Ok(())
    }

    async fn perform(&mut self, _action: &str, _data: &Params, _sub: &mut Subscription<CableUser>) -> ChannelResult<bool> {
        Ok(false)
    }
}
