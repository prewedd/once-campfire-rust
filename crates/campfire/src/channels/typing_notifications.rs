//! `TypingNotificationsChannel` (reference/app/channels/typing_notifications_channel.rb).
use campfire_cable::{Channel, ChannelError, ChannelResult, Params, Subscription};
use campfire_db::{Database, Room};
use serde::Serialize;

use super::{CableUser, room, room_gid};

pub struct TypingNotificationsChannel {
    db: Database,
    room: Option<Room>,
    /// The thread being typed in, if any (plans/threads.md): its typing stays in the thread.
    thread_id: Option<i64>,
}

impl TypingNotificationsChannel {
    pub fn new(db: Database) -> Self {
        Self { db, room: None, thread_id: None }
    }

    /// `room::subscribe`, streaming from the thread's own stream when `params[:thread_id]` names
    /// one, so the room and each thread hear only their own typing.
    async fn subscribe(&mut self, sub: &mut Subscription<CableUser>) -> ChannelResult {
        self.room = room::find_room(&self.db, sub).await?;
        self.thread_id = sub.param("thread_id").as_ref().and_then(room::cast_id).filter(|id| *id > 0);
        match &self.room {
            Some(room) => {
                let streamables = streamables(room, self.thread_id);
                sub.stream_for(&streamables.iter().map(String::as_str).collect::<Vec<_>>());
            }
            None => sub.reject(),
        }
        Ok(())
    }

    /// `broadcast_to @room, action:, user: current_user.slice(:id, :name)`.
    fn broadcast(&self, action: &'static str, sub: &Subscription<CableUser>) -> ChannelResult {
        // `@room` is only nil when `subscribed` failed, and Rails raises NoMethodError.
        let room = self.room.as_ref().ok_or_else(|| ChannelError("undefined method 'to_gid_param' for nil".into()))?;
        let user = sub.current_user();
        let payload = Payload { action, user: UserAttributes { id: user.id, name: &user.name } };
        let streamables = streamables(room, self.thread_id);
        sub.broadcast_to(&streamables.iter().map(String::as_str).collect::<Vec<_>>(), &payload);
        Ok(())
    }
}

/// The room's typing stream (as in Rails), or a thread's within it.
fn streamables(room: &Room, thread_id: Option<i64>) -> Vec<String> {
    let mut streamables = vec![room_gid(room).to_param()];
    streamables.extend(thread_id.map(|id| format!("thread_{id}")));
    streamables
}

#[derive(Serialize)]
pub struct Payload<'a> {
    pub action: &'static str,
    pub user: UserAttributes<'a>,
}

#[derive(Serialize)]
pub struct UserAttributes<'a> {
    pub id: i64,
    pub name: &'a str,
}

#[async_trait::async_trait]
impl Channel<CableUser> for TypingNotificationsChannel {
    async fn subscribed(&mut self, sub: &mut Subscription<CableUser>) -> ChannelResult {
        self.subscribe(sub).await
    }

    async fn perform(&mut self, action: &str, _data: &Params, sub: &mut Subscription<CableUser>) -> ChannelResult<bool> {
        if sub.rejected() {
            return Ok(false);
        }
        match action {
            "start" => self.broadcast("start", sub)?,
            "stop" => self.broadcast("stop", sub)?,
            "subscribed" => self.subscribe(sub).await?,
            _ => return Ok(false),
        }
        Ok(true)
    }
}
