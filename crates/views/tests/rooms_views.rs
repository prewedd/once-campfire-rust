//! DOM parity of the room views with the reference app (goldens in tests/golden/b).

mod messages_support;

use askama::Template;
use campfire_views::rooms::{self, ClosedFormView, DirectEditView, InvolvementView, OpenFormView, RefreshView, ShowView};
use messages_support::golden;

fn show(name: &str) {
    let g = golden(name);
    let show: ShowView = g.input();
    g.assert_dom(&g.render(|ctx| rooms::Show { ctx, show: &show }.render().unwrap()));
}

/// A room page as it's served, recorded, is what a plain render gives, and each of its messages is
/// a fragment the kit gets as it is: rendered into the fragment cache first, then read from it.
#[test]
fn show_recorded_keeps_every_message_as_a_fragment() {
    use campfire_views::fragment_cache::{self, FragmentCache};
    use campfire_views::layouts;

    let g = golden("rooms_show_member");
    let show: ShowView = g.input();
    assert!(!show.messages.is_empty());
    let cache = FragmentCache::new(fragment_cache::DEFAULT_MAX_BYTES);
    for round in ["cold", "warm"] {
        g.render(|ctx| {
            fragment_cache::with(&cache, || {
                let page = || rooms::Show { ctx, show: &show };
                let recorded = campfire_views::render_sized!(page()).unwrap();
                let plain = page().render().unwrap();
                assert_eq!(recorded.to_string(), plain, "{round}");
                assert_eq!(recorded.fragments().len(), show.messages.len(), "{round}");

                let framed = layouts::frame(ctx, page().as_head(), page().as_content()).unwrap();
                let plain_frame = layouts::FrameLayout {
                    ctx,
                    head: askama::filters::Safe(page().as_head().render().unwrap()),
                    content: askama::filters::Safe(page().as_content().render().unwrap().into()),
                }
                .render()
                .unwrap();
                assert_eq!(framed.to_string(), plain_frame, "{round}");
                assert_eq!(framed.fragments().len(), show.messages.len(), "{round}");
                plain
            })
        });
    }
}

#[test]
fn show_closed_room() {
    show("rooms_show_closed");
}

#[test]
fn show_original_room_with_invitation() {
    show("rooms_show_original");
}

#[test]
fn show_direct_room() {
    show("rooms_show_direct");
}

#[test]
fn show_room_as_member() {
    show("rooms_show_member");
}

#[test]
fn show_room_around_message() {
    show("rooms_show_at_message");
}

/// The channel forms are laid out as Slack's (plans/redesign.md, README "Known differences"), so
/// they're checked for what they offer rather than against the reference's markup.
fn assert_channel_form(name: &str, html: &str, private: bool) {
    let administer = !name.ends_with("_member");
    assert_eq!(html.contains(r#"name="room[name]""#), administer, "{name}: only those who administer it rename a channel");
    assert_eq!(html.contains(r#"class="ws-choices""#), administer, "{name}: the public/private choice");
    assert_eq!(html.contains(r#"name="user_ids[]""#), private && administer, "{name}: private channels pick their members");
    if administer {
        let submit = if name.contains("_new") { "Create channel" } else { "Save changes" };
        assert!(html.contains(submit), "{name}: no {submit:?}");
        assert_eq!(html.contains("Delete channel"), name.contains("_edit"), "{name}");
    }
}

#[test]
fn new_open_room() {
    let g = golden("rooms_opens_new");
    let form: OpenFormView = g.input();
    assert_channel_form(&g.name, &g.render(|ctx| rooms::OpensNew { ctx, form: &form }.render().unwrap()), false);
}

#[test]
fn edit_open_room() {
    for name in ["rooms_opens_edit", "rooms_opens_edit_member"] {
        let g = golden(name);
        let form: OpenFormView = g.input();
        assert_channel_form(name, &g.render(|ctx| rooms::OpensEdit { ctx, form: &form }.render().unwrap()), false);
    }
}

#[test]
fn new_closed_room() {
    let g = golden("rooms_closeds_new");
    let form: ClosedFormView = g.input();
    assert_channel_form(&g.name, &g.render(|ctx| rooms::ClosedsNew { ctx, form: &form }.render().unwrap()), true);
}

#[test]
fn edit_closed_room() {
    for name in ["rooms_closeds_edit", "rooms_closeds_edit_member"] {
        let g = golden(name);
        let form: ClosedFormView = g.input();
        assert_channel_form(name, &g.render(|ctx| rooms::ClosedsEdit { ctx, form: &form }.render().unwrap()), true);
    }
}

#[test]
fn new_direct_room() {
    // "New message", Slack's way (plans/redesign.md): checked for what it offers.
    let g = golden("rooms_directs_new");
    let html = g.render(|ctx| rooms::DirectsNew { ctx }.render().unwrap());
    for part in [r#"<turbo-frame id="direct_rooms_control" target="_top">"#, r#"action="/rooms/directs""#, r#"data-controller="autocomplete""#, r#"name="user_ids[]""#, ">To:</label>", ">Start</button>"] {
        assert!(html.contains(part), "no {part:?}: {html}");
    }
}

#[test]
fn edit_direct_room() {
    let g = golden("rooms_directs_edit");
    let edit: DirectEditView = g.input();
    let html = g.render(|ctx| rooms::DirectsEdit { ctx, edit: &edit }.render().unwrap());
    assert_eq!(html.matches(r#"<li class="ws-row member">"#).count(), edit.users.len(), "{html}");
    assert!(html.contains(&format!("/rooms/directs/{}\"", edit.room_id)) && html.contains("Delete conversation"), "{html}");
}

#[test]
fn involvement() {
    for name in ["rooms_involvements_show", "rooms_involvements_show_direct"] {
        let g = golden(name);
        let involvement: InvolvementView = g.input();
        g.assert_content(&g.render(|ctx| rooms::InvolvementShow { ctx, involvement: &involvement }.render().unwrap()));
    }
}

#[test]
fn refresh_stream() {
    let g = golden("rooms_refreshes_show");
    let refresh: RefreshView = g.input();
    g.assert_dom(&g.render(|ctx| rooms::RefreshShow { ctx, refresh: &refresh }.render().unwrap()));
}

#[test]
fn display_names() {
    let names = |n: &[&str]| n.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(rooms::room_display_name(Some("HQ"), false, &[], None), "HQ");
    assert_eq!(rooms::room_display_name(None, true, &names(&["Jason", "JZ"]), Some("David")), "Jason and JZ");
    assert_eq!(rooms::room_display_name(None, true, &[], Some("David")), "David");
}
