//! Views for `reference/app/views/welcome`.

use askama::Template;

use crate::ViewContext;
use crate::helpers as h;
use crate::layouts::Page;

/// `welcome/show.html.erb`: shown to users who aren't in any room yet.
#[derive(Template)]
#[template(path = "welcome/show.html", blocks = ["head", "content"])]
pub struct Show<'a> {
    pub ctx: &'a ViewContext<'a>,
    /// `Current.user.name`.
    pub current_user_name: String,
}

impl Page for Show<'_> {
    fn page_title(&self) -> Option<String> {
        Some("No rooms yet".into())
    }
    fn body_class(&self) -> Option<&str> {
        Some("sidebar")
    }
}
