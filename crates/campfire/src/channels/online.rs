//! `OnlineChannel`: who's online across the account, for the presence dots (plans/redesign.md).
//!
//! Every open page subscribes once. A person is online while one of their pages is visible, away
//! while all of them are hidden, and offline once none is open; changes go out on the `online`
//! stream as `{ user_id, status }`, and a new subscriber gets everyone's status first. A person
//! goes offline a few seconds after their last page closes, so moving between pages doesn't flicker.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use campfire_cable::{Channel, ChannelResult, Params, Subscription};
use serde::Serialize;

use super::{Cable, CableUser};

pub const STREAM: &str = "online";
const OFFLINE_GRACE: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Online,
    Away,
    Offline,
}

/// Each person's open pages: subscription key → hidden.
#[derive(Default)]
struct Registry {
    pages: HashMap<i64, HashMap<u64, bool>>,
}

impl Registry {
    fn status(&self, user_id: i64) -> Status {
        match self.pages.get(&user_id) {
            None => Status::Offline,
            Some(pages) if pages.values().any(|hidden| !hidden) => Status::Online,
            Some(_) => Status::Away,
        }
    }

    /// Sets a page's visibility (adding it), and returns the person's status before and after.
    fn set(&mut self, user_id: i64, key: u64, hidden: bool) -> (Status, Status) {
        let before = self.status(user_id);
        self.pages.entry(user_id).or_default().insert(key, hidden);
        (before, self.status(user_id))
    }

    fn remove(&mut self, user_id: i64, key: u64) -> (Status, Status) {
        let before = self.status(user_id);
        if let Some(pages) = self.pages.get_mut(&user_id) {
            pages.remove(&key);
            if pages.is_empty() {
                self.pages.remove(&user_id);
            }
        }
        (before, self.status(user_id))
    }

    fn snapshot(&self) -> HashMap<i64, Status> {
        self.pages.keys().map(|user_id| (*user_id, self.status(*user_id))).collect()
    }
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(Mutex::default);
static NEXT_KEY: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize)]
struct Change {
    user_id: i64,
    status: Status,
}

#[derive(Serialize)]
struct Snapshot {
    snapshot: HashMap<i64, Status>,
}

fn announce(server: &Cable, user_id: i64, (before, after): (Status, Status)) {
    if before == after {
        return;
    }
    if after == Status::Offline {
        // Unless they're back (another page, a reload) by then.
        let server = server.clone();
        tokio::spawn(async move {
            tokio::time::sleep(OFFLINE_GRACE).await;
            if REGISTRY.lock().unwrap().status(user_id) == Status::Offline {
                server.broadcast(STREAM, &Change { user_id, status: Status::Offline });
            }
        });
    } else {
        server.broadcast(STREAM, &Change { user_id, status: after });
    }
}

pub struct OnlineChannel {
    key: u64,
    user_id: Option<i64>,
}

impl OnlineChannel {
    pub fn new() -> Self {
        Self { key: NEXT_KEY.fetch_add(1, Ordering::Relaxed), user_id: None }
    }

    fn set_hidden(&self, sub: &Subscription<CableUser>, hidden: bool) {
        let user_id = sub.current_user().id;
        let change = REGISTRY.lock().unwrap().set(user_id, self.key, hidden);
        announce(sub.server(), user_id, change);
    }
}

#[async_trait::async_trait]
impl Channel<CableUser> for OnlineChannel {
    async fn subscribed(&mut self, sub: &mut Subscription<CableUser>) -> ChannelResult {
        let user_id = sub.current_user().id;
        self.user_id = Some(user_id);
        self.set_hidden(sub, false);
        sub.stream_from(STREAM);
        let snapshot = REGISTRY.lock().unwrap().snapshot();
        sub.transmit(&Snapshot { snapshot });
        Ok(())
    }

    async fn unsubscribed(&mut self, sub: &mut Subscription<CableUser>) -> ChannelResult {
        if let Some(user_id) = self.user_id.take() {
            let change = REGISTRY.lock().unwrap().remove(user_id, self.key);
            announce(sub.server(), user_id, change);
        }
        Ok(())
    }

    async fn perform(&mut self, action: &str, _data: &Params, sub: &mut Subscription<CableUser>) -> ChannelResult<bool> {
        if sub.rejected() || self.user_id.is_none() {
            return Ok(false);
        }
        match action {
            "away" => self.set_hidden(sub, true),
            "active" => self.set_hidden(sub, false),
            _ => return Ok(false),
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_while_a_page_is_visible_away_while_all_are_hidden() {
        let mut registry = Registry::default();
        assert_eq!(registry.set(7, 1, false), (Status::Offline, Status::Online));
        assert_eq!(registry.set(7, 2, true), (Status::Online, Status::Online));
        assert_eq!(registry.set(7, 1, true), (Status::Online, Status::Away));
        assert_eq!(registry.remove(7, 2), (Status::Away, Status::Away));
        assert_eq!(registry.remove(7, 1), (Status::Away, Status::Offline));
        assert!(registry.snapshot().is_empty());
        assert_eq!(registry.remove(7, 1), (Status::Offline, Status::Offline), "twice is harmless");
    }
}
