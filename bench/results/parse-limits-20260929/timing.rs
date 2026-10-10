//! Times message_presentation and to_plain_text on hostile and ordinary bodies. Copied into
//! crates/richtext/examples/ to run: `cargo run --release -p campfire_richtext --example timing`

use std::time::{Duration, Instant};

use campfire_richtext::{AttachableResolver, GidLookup, RenderContext, SignedLookup, message_presentation, to_plain_text};

struct NoRecords;

impl AttachableResolver for NoRecords {
    fn locate_signed(&self, _: &str) -> SignedLookup {
        SignedLookup::Invalid
    }
    fn find_gid(&self, _: &str) -> GidLookup {
        GidLookup::NotFound
    }
}

fn main() {
    let ctx = RenderContext { resolver: &NoRecords, request_host: Some("once.campfire.test".into()) };
    let attributes = |n: usize| format!("<b {}>x</b>", (1..=n).map(|i| format!("a{i}=1")).collect::<Vec<_>>().join(" "));
    let html_tags = |n: usize| -> String {
        (0..n).map(|t| format!("<html {}>", (1..=400).map(|i| format!("a{}", t * 400 + i)).collect::<Vec<_>>().join(" "))).collect()
    };
    let bodies = [
        ("small message", "<p>Hello <b>world</b>, see https://example.com</p>".to_string()),
        ("paragraphs", "<p>Some text here and there.</p>".repeat(216 * 1024 / 32)),
        ("<div> x 400", "<div>".repeat(400)),
        ("<div> x 20k", "<div>".repeat(20_000)),
        ("<div> x 80k", "<div>".repeat(80_000)),
        ("<a><b> x 80k", "<a><b>".repeat(80_000)),
        ("<a><div><div> x 30k", "<a><div><div>".repeat(30_000)),
        ("<b> with 32k attributes", attributes(32_000)),
        ("<table> and <br> x 120k", format!("<table>{}", "<br>".repeat(120_000))),
        ("<html> x 200, each with 400 new attributes", html_tags(200)),
        ("<div> x 401 then <a><b> to 16 MB", format!("{}{}", "<div>".repeat(401), "<a><b>".repeat(16 * 1024 * 1024 / 6))),
        ("<div> x 401 then 16 MB of text", format!("{}{}", "<div>".repeat(401), "x".repeat(16 * 1024 * 1024))),
        ("<div> x 401 then a 16 MB comment", format!("{}<!--{}", "<div>".repeat(401), "x".repeat(16 * 1024 * 1024))),
        ("<div> x 401 then a 16 MB tag name", format!("{}<{}", "<div>".repeat(401), "x".repeat(16 * 1024 * 1024))),
        ("<div> x 401 then a 16 MB attribute value", format!("{}<p title=\"{}\">", "<div>".repeat(401), "x".repeat(16 * 1024 * 1024))),
    ];
    for (name, body) in bodies {
        // Best of a few runs for the quick ones
        let runs = if body.len() < 4096 { 200 } else { 3 };
        let (presented, html) = best_of(runs, || message_presentation(&body, &ctx).map(|h| h.len()));
        let (indexed, text) = best_of(runs, || to_plain_text(&body, &ctx).map(|t| t.len()));
        println!("{name} ({} bytes): presentation {presented:?} ({html:?}), plain text {indexed:?} ({text:?})", body.len());
    }
}

fn best_of<T>(runs: usize, f: impl Fn() -> T) -> (Duration, T) {
    let mut best = Duration::MAX;
    let mut result = None;
    for _ in 0..runs {
        let started = Instant::now();
        let value = f();
        best = best.min(started.elapsed());
        result = Some(value);
        // A run this slow says enough once
        if best > Duration::from_secs(1) {
            break;
        }
    }
    (best, result.unwrap())
}
