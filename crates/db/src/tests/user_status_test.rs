//! Custom statuses (plans/redesign.md): set, defaulted, cleared, and gone once past their time.

use super::*;
use crate::UserStatus;

#[test]
fn a_status_is_trimmed_and_gets_the_default_emoji() {
    let t = TestDb::new();
    let david = id("david");
    let status = t.write(move |tx| UserStatus::set(tx, david, " ", "  In a meeting  ", None, 1_000)).unwrap();
    assert_eq!((status.emoji.as_str(), status.text.as_str()), (UserStatus::DEFAULT_EMOJI, "In a meeting"));
    assert_eq!(t.read(move |c| UserStatus::for_user(c, david, 1_000)), Some(status));
}

#[test]
fn setting_it_again_replaces_it_and_blank_clears_it() {
    let t = TestDb::new();
    let david = id("david");
    t.write(move |tx| UserStatus::set(tx, david, "🚌", "Commuting", None, 1_000));
    t.write(move |tx| UserStatus::set(tx, david, "🌴", "Vacationing", None, 1_001));
    assert_eq!(t.read(|c| UserStatus::current(c, 1_001)).iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), vec!["Vacationing"]);
    assert_eq!(t.write(move |tx| UserStatus::set(tx, david, "", " ", None, 1_002)), None);
    assert!(t.read(|c| UserStatus::current(c, 1_002)).is_empty());
}

#[test]
fn a_status_clears_itself_at_its_time() {
    let t = TestDb::new();
    let (david, jason) = (id("david"), id("jason"));
    t.write(move |tx| UserStatus::set(tx, david, "📅", "In a meeting", Some(2_000), 1_000));
    t.write(move |tx| UserStatus::set(tx, jason, "🏡", "Working remotely", None, 1_000));
    assert_eq!(t.read(|c| UserStatus::current(c, 1_999)).len(), 2);
    assert_eq!(t.read(|c| UserStatus::current(c, 2_000)).iter().map(|s| s.user_id).collect::<Vec<_>>(), vec![jason]);
    assert_eq!(t.read(move |c| UserStatus::for_user(c, david, 2_000)), None);
}
