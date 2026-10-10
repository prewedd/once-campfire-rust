//! Views for `reference/app/views/first_runs`.

use askama::Template;

use crate::ViewContext;
use crate::helpers::{self as h, filters};
use crate::layouts::Page;

/// `first_runs/show.html.erb`: account setup, shown until the first user exists.
#[derive(Template)]
#[template(path = "first_runs/show.html", blocks = ["head", "content"])]
pub struct Show<'a> {
    pub ctx: &'a ViewContext<'a>,
}

impl Page for Show<'_> {
    fn page_title(&self) -> Option<String> {
        Some("Set up Campfire".into())
    }
    fn body_class(&self) -> Option<&str> {
        Some("signup")
    }
}
