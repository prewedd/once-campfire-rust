//! The user view-model shared by every users/accounts/autocompletable template.

use crate::helpers as h;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    Member,
    Administrator,
    Bot,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Member => "member",
            Role::Administrator => "administrator",
            Role::Bot => "bot",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Active,
    Deactivated,
    Banned,
}

/// A `User` row as the views see it.
#[derive(Clone, Debug, Default)]
pub struct UserSummary {
    pub id: i64,
    pub name: String,
    pub bio: Option<String>,
    pub email_address: Option<String>,
    pub role: Role,
    pub status: Status,
    /// `fresh_user_avatar_path(user)`.
    pub avatar_path: String,
}

impl UserSummary {
    pub fn active(&self) -> bool {
        self.status == Status::Active
    }
    pub fn banned(&self) -> bool {
        self.status == Status::Banned
    }
    pub fn deactivated(&self) -> bool {
        self.status == Status::Deactivated
    }
    pub fn bot(&self) -> bool {
        self.role == Role::Bot
    }
    pub fn administrator(&self) -> bool {
        self.role == Role::Administrator
    }

    /// `User#title`.
    pub fn title(&self) -> String {
        h::user_title(&self.name, self.bio.as_deref())
    }

    /// `User#initials`.
    pub fn initials(&self) -> String {
        h::initials(&self.name)
    }

    pub fn avatar(&self) -> h::AvatarUser {
        h::AvatarUser { id: self.id, title: self.title(), avatar_path: self.avatar_path.clone() }
    }

    /// `name.split(' ')[0]` (awk-style split on ASCII whitespace).
    pub fn first_name(&self) -> &str {
        self.name_parts().next().unwrap_or("")
    }

    /// `name.split(' ')`.
    pub fn name_parts(&self) -> impl Iterator<Item = &str> {
        self.name.split(|c: char| c.is_ascii_whitespace()).filter(|part| !part.is_empty())
    }
}
