//! Paused notifications (plans/redesign.md): paused until a time, then not.

use super::*;
use crate::NotificationPause;

#[test]
fn a_pause_lasts_until_its_time_or_until_resumed() {
    let t = TestDb::new();
    let (david, jason) = (id("david"), id("jason"));
    t.write(move |tx| NotificationPause::pause(tx, david, 2_000));
    t.write(move |tx| NotificationPause::pause(tx, jason, 5_000));
    assert_eq!(t.read(move |c| NotificationPause::until(c, david, 1_000)), Some(2_000));
    assert_eq!(t.read(|c| NotificationPause::paused_user_ids(c, 1_999)).len(), 2);
    assert_eq!(t.read(|c| NotificationPause::paused_user_ids(c, 2_000)), vec![jason], "over at its time");
    t.write(move |tx| NotificationPause::resume(tx, jason));
    assert!(t.read(|c| NotificationPause::paused_user_ids(c, 2_000)).is_empty());
}
