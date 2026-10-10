//! The search page, laid out as Slack's (plans/redesign.md, README "Known differences"): checked
//! for what it offers, from the reference's inputs (goldens in tests/golden/b).

mod messages_support;

use askama::Template;
use campfire_views::searches::{self, IndexView};
use messages_support::golden;

#[test]
fn index_with_results() {
    let g = golden("searches_index");
    let index: IndexView = g.input();
    let html = g.render(|ctx| searches::Index { ctx, index: &index }.render().unwrap());
    let query = index.query.as_deref().unwrap();
    assert!(html.contains(&format!("Results for “{query}”")), "{html}");
    assert!(html.contains(&format!("{} message", index.messages.len())), "{html}");
    assert!(html.contains(r#"class="ws-search""#) && html.contains(r#"name="q""#), "the search box is in the header");
    assert_eq!(html.matches(r#"class="message "#).count() + html.matches(r#"class="message""#).count(), index.messages.len(), "{html}");
    assert!(!html.contains("searches/_recents"), "{html}");
}

#[test]
fn index_without_query() {
    let g = golden("searches_index_empty");
    let index: IndexView = g.input();
    let html = g.render(|ctx| searches::Index { ctx, index: &index }.render().unwrap());
    assert!(html.contains("Search messages") && !html.contains("Results for"), "{html}");
    assert_eq!(html.matches(r#"<span class="ws-row__name">"#).count(), index.recent_searches.len(), "{html}");
    assert!(!html.contains(r#"pizza & "pie""#), "recent searches are escaped");
    assert_eq!(html.contains("Clear recent searches"), !index.recent_searches.is_empty());
}
