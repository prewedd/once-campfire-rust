//! `ActiveRecord::SignedId` (`signed_id(purpose:, expires_in:)` / `find_signed`).
//!
//! The verifier is `Rails.application.message_verifiers["active_record/signed_id"]` with the
//! legacy options prepended (`use_legacy_signed_id_verifier` defaults to `:generate_and_verify`):
//! it generates with SHA256, `::JSON`, URL-safe Base64, and falls back when reading to the
//! app-wide default (SHA1, `:json_allow_marshal`, strict Base64). The purpose is
//! `"<base class name underscored>/<purpose>"`, e.g. `user/avatar`, or just `user`.
use jiff::Timestamp;
use serde_json::Value;

use crate::Secrets;
use crate::message_verifier::{Digest, Encoding, MessageVerifier, Serializer};

pub const SALT: &str = "active_record/signed_id";

/// `model_name` is the record's *base* class name, e.g. "User" or "Room" (not "Rooms::Open").
pub fn generate(secrets: &Secrets, model_name: &str, id: i64, purpose: Option<&str>, expires_at: Option<Timestamp>) -> String {
    verifier(secrets).generate(&Value::from(id), Some(&combine_purposes(model_name, purpose)), expires_at)
}

/// `find_signed`'s verification step: the id to look up, or `None`.
pub fn verify(secrets: &Secrets, model_name: &str, signed_id: &str, purpose: Option<&str>, now: Timestamp) -> Option<i64> {
    match verifier(secrets).verify(signed_id, Some(&combine_purposes(model_name, purpose)), now).ok()? {
        Value::Number(n) => n.as_i64(),
        // `find_by(id: "7")` casts the string.
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

pub fn verifier(secrets: &Secrets) -> MessageVerifier {
    let secret = secrets.key_generator.generate_key(SALT, 64);
    let fallback =
        MessageVerifier::new(secret.clone(), Digest::Sha1, Encoding::Strict, Serializer::JsonWithFallback { allow_marshal: true });
    MessageVerifier::new(secret, Digest::Sha256, Encoding::UrlSafe, Serializer::Json).fall_back_to(fallback)
}

/// `combine_signed_id_purposes`: `[base_class.name.underscore, purpose.to_s].compact_blank.join("/")`.
pub fn combine_purposes(model_name: &str, purpose: Option<&str>) -> String {
    [underscore(model_name), purpose.unwrap_or("").to_string()]
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// `String#underscore` for class names: `Rooms::Open` → `rooms/open`, `WebPush` → `web_push`.
fn underscore(name: &str) -> String {
    let name = name.replace("::", "/");
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            let previous = i.checked_sub(1).map(|j| chars[j]);
            let next = chars.get(i + 1);
            let after_lower_or_digit = previous.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit());
            let acronym_end = previous.is_some_and(|p| p.is_ascii_uppercase()) && next.is_some_and(|n| n.is_ascii_lowercase());
            if after_lower_or_digit || acronym_end {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(if c == '-' { '_' } else { c });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combines_purposes() {
        assert_eq!(combine_purposes("User", Some("avatar")), "user/avatar");
        assert_eq!(combine_purposes("User", None), "user");
        assert_eq!(combine_purposes("Rooms::Open", Some("")), "rooms/open");
        assert_eq!(combine_purposes("HTTPRequest", Some("x")), "http_request/x");
    }
}
