//! The workspace redesign (plans/redesign.md): this app's own markup, which the parity goldens
//! leave out.

mod messages_support;

use askama::Template;
use campfire_views::messages::{self, MessageView};
use campfire_views::rooms::{self, ShowView};
use campfire_views::threads;
use messages_support::golden;

#[test]
fn the_composer_has_emoji_and_says_where_youre_writing() {
    let g = golden("rooms_show_member");
    let show: ShowView = g.input();
    let html = g.render(|ctx| rooms::Show { ctx, show: &show }.render().unwrap());
    assert!(html.contains(&format!(r#"placeholder="Message #{}""#, show.room.display_name)), "{html}");
    assert!(html.contains(r#"class="btn btn--borderless txt-small flex-item-no-shrink composer__emoji-btn" type="button" data-controller="emoji" data-action="emoji#insert"#));
    assert!(html.contains(r#"<lexxy-prompt trigger=":" name="emoji" src="/assets/emoji/prompt.html" insert-editable-text"#));
}

#[test]
fn a_ping_names_who_youre_writing_to() {
    let g = golden("rooms_show_direct");
    let show: ShowView = g.input();
    let html = g.render(|ctx| rooms::Show { ctx, show: &show }.render().unwrap());
    assert!(html.contains(&format!(r#"placeholder="Message {}""#, show.room.display_name)), "{html}");
}

#[test]
fn a_thread_asks_for_a_reply() {
    let g = golden("rooms_show_member");
    let room: ShowView = g.input();
    let show = threads::ShowView {
        room: room.room.clone(),
        parent_id: 42,
        updated_at: room.updated_at,
        user: room.user.clone(),
        messages: room.messages.clone(),
        messages_stream_name: room.messages_stream_name.clone(),
    };
    let html = g.render(|ctx| threads::Show { ctx, show: &show }.render().unwrap());
    assert!(html.contains(r#"placeholder="Reply…""#), "{html}");
}

#[test]
fn messages_offer_quick_reactions_and_the_picker() {
    let g = golden("messages_show_text");
    let message: MessageView = g.input();
    let html = g.render(|ctx| messages::MessagePartial { ctx, message: &message }.render().unwrap());
    let toolbar = format!(r#"<div class="message__toolbar-reactions" data-controller="emoji" data-emoji-boosts-url-value="/messages/{}/boosts">"#, message.id);
    assert!(html.contains(&toolbar), "{html}");
    for (character, _) in messages::QUICK_REACTIONS {
        assert!(html.contains(&format!(r#"data-emoji-content-param="{character}""#)));
    }
    assert!(html.contains(r#"data-action="emoji#pickReaction"#));
}

#[test]
fn editing_a_message_offers_emoji_too() {
    let g = golden("messages_edit_text");
    let edit: messages::EditView = g.input();
    let html = g.render(|ctx| messages::Edit { ctx, edit: &edit }.render().unwrap());
    assert!(html.contains(r#"<lexxy-prompt trigger=":" name="emoji""#), "{html}");
}

#[test]
fn an_edited_message_says_so() {
    let g = golden("messages_show_text");
    let mut message: MessageView = g.input();
    let html = g.render(|ctx| messages::MessagePartial { ctx, message: &message }.render().unwrap());
    assert!(!html.contains("(edited)"), "{html}");
    message.edited = true;
    let html = g.render(|ctx| messages::MessagePartial { ctx, message: &message }.render().unwrap());
    assert!(html.contains(r#"<span class="message__edited">(edited)</span>"#), "{html}");
}
