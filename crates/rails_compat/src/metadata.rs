//! `ActiveSupport::Messages::Metadata` and `SerializerWithFallback`: how a value and its purpose
//! and expiry are packed into the bytes that get signed or encrypted.
//!
//! Two envelopes exist. When the serializer is one Rails trusts for metadata (JSON, or the
//! `:json_allow_marshal` fallback serializer) it is `{"_rails":{"data":<value>,"exp":..,"pur":..}}`
//! in that serializer. With the cookie jars' `NullSerializer` (and in Rails 7.0) it's the legacy
//! "dual-serialized" one, `{"_rails":{"message":"<base64 of the dumped value>","exp":..,"pur":..}}`,
//! which always carries `exp` and `pur`, as `null` when unset.
use jiff::Timestamp;
use serde_json::Value;

use crate::{Error, encoding, json, marshal};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Serializer {
    /// `ActiveSupport::MessageEncryptor::NullSerializer`: the value is a string of already
    /// serialized bytes (what the cookie jars sign and encrypt). Uses the legacy envelope.
    Null,
    /// The `::JSON` module (`JSON.dump`/`JSON.load`), as signed ids and Turbo stream names use.
    Json,
    /// `SerializerWithFallback[:json]` or `[:json_allow_marshal]` with `ActiveSupport::JSON`: the
    /// app's default `message_serializer` under `load_defaults` 7.1+.
    JsonWithFallback { allow_marshal: bool },
}

impl Serializer {
    pub(crate) fn dump(&self, value: &Value) -> Vec<u8> {
        match self {
            Serializer::Null => match value {
                Value::String(s) => s.as_bytes().to_vec(),
                other => panic!("the null serializer only signs strings, got {other}"),
            },
            Serializer::Json => json::generate(value).into_bytes(),
            Serializer::JsonWithFallback { .. } => json::encode(value).into_bytes(),
        }
    }

    pub(crate) fn load(&self, bytes: &[u8]) -> Result<Value, Error> {
        match self {
            Serializer::Null => String::from_utf8(bytes.to_vec()).map(Value::String).map_err(|_| Error::InvalidMessage),
            // JSON.load("") is nil.
            Serializer::Json if bytes.is_empty() => Ok(Value::Null),
            Serializer::Json => json::parse(bytes).ok_or(Error::InvalidMessage),
            Serializer::JsonWithFallback { allow_marshal } => {
                if bytes.starts_with(marshal::SIGNATURE) {
                    if !allow_marshal {
                        return Err(Error::InvalidMessage);
                    }
                    let string = marshal::load_string(bytes).ok_or(Error::InvalidMessage)?;
                    Ok(Value::String(String::from_utf8_lossy(&string).into_owned()))
                } else {
                    json::parse(bytes).ok_or(Error::InvalidMessage)
                }
            }
        }
    }

    pub(crate) fn encode_json(&self, value: &Value) -> String {
        match self {
            Serializer::Json => json::generate(value),
            _ => json::encode(value),
        }
    }

    fn uses_envelope(&self) -> bool {
        !matches!(self, Serializer::Null)
    }
}

/// `Time#iso8601(3)` in UTC: `2046-01-01T12:00:00.000Z` (fraction truncated, not rounded).
pub fn iso8601_millis(time: Timestamp) -> String {
    let millis = time.subsec_nanosecond().div_euclid(1_000_000);
    let seconds = Timestamp::from_second(time.as_second()).expect("whole seconds of a valid timestamp");
    format!("{}.{millis:03}Z", seconds.strftime("%Y-%m-%dT%H:%M:%S"))
}

pub(crate) fn serialize_with_metadata(
    serializer: Serializer,
    value: &Value,
    purpose: Option<&str>,
    expires_at: Option<Timestamp>,
) -> Vec<u8> {
    serialize_dumped_with_metadata(serializer, &serializer.dump(value), purpose, expires_at)
}

/// Like [`serialize_with_metadata`] for a value the caller already dumped with `serializer`
/// (so the caller controls key order and escaping).
pub(crate) fn serialize_dumped_with_metadata(
    serializer: Serializer,
    dumped: &[u8],
    purpose: Option<&str>,
    expires_at: Option<Timestamp>,
) -> Vec<u8> {
    if purpose.is_none() && expires_at.is_none() {
        return dumped.to_vec();
    }

    let expiry = expires_at.map(|t| Value::String(iso8601_millis(t)));
    let purpose = purpose.map(|p| Value::String(p.to_string()));

    if serializer.uses_envelope() {
        let mut out = format!(r#"{{"_rails":{{"data":{}"#, String::from_utf8_lossy(dumped));
        if let Some(expiry) = expiry {
            out.push_str(&format!(r#","exp":{}"#, serializer.encode_json(&expiry)));
        }
        if let Some(purpose) = purpose {
            out.push_str(&format!(r#","pur":{}"#, serializer.encode_json(&purpose)));
        }
        out.push_str("}}");
        out.into_bytes()
    } else {
        let message = Value::String(encoding::strict_encode(dumped));
        format!(
            r#"{{"_rails":{{"message":{},"exp":{},"pur":{}}}}}"#,
            json::encode(&message),
            json::encode(&expiry.unwrap_or(Value::Null)),
            json::encode(&purpose.unwrap_or(Value::Null)),
        )
        .into_bytes()
    }
}

/// `deserialize_with_metadata`. `decode_legacy_message` decodes the base64 inside a legacy
/// envelope: the verifier accepts either alphabet, the encryptor only strict Base64.
pub(crate) fn deserialize_with_metadata(
    serializer: Serializer,
    bytes: &[u8],
    purpose: Option<&str>,
    now: Timestamp,
    decode_legacy_message: fn(&str) -> Option<Vec<u8>>,
) -> Result<Value, Error> {
    if bytes.starts_with(br#"{"_rails":{"message":"#) {
        let envelope = json::parse(bytes).ok_or(Error::InvalidSignature)?;
        let rails = extract(&envelope, purpose, now)?;
        let message = rails.get("message").and_then(Value::as_str).ok_or(Error::InvalidSignature)?;
        let dumped = decode_legacy_message(message).ok_or(Error::InvalidSignature)?;
        serializer.load(&dumped)
    } else {
        let value = serializer.load(bytes)?;
        if value.get("_rails").is_some() && value.is_object() {
            Ok(extract(&value, purpose, now)?.get("data").cloned().unwrap_or(Value::Null))
        } else if purpose.is_none() {
            Ok(value)
        } else {
            Err(Error::PurposeMismatch)
        }
    }
}

/// `extract_from_metadata_envelope`: `exp` is expired when `now >= exp`; purposes compare with
/// `to_s`, so a missing `pur` matches no purpose.
fn extract<'a>(envelope: &'a Value, purpose: Option<&str>, now: Timestamp) -> Result<&'a serde_json::Map<String, Value>, Error> {
    let rails = envelope.get("_rails").and_then(Value::as_object).ok_or(Error::InvalidMessage)?;

    match rails.get("exp") {
        None | Some(Value::Null) => {}
        Some(Value::String(exp)) => {
            let exp: Timestamp = exp.parse().map_err(|_| Error::InvalidMessage)?;
            if now >= exp {
                return Err(Error::Expired);
            }
        }
        Some(_) => return Err(Error::InvalidMessage),
    }

    if ruby_to_s(rails.get("pur")) != purpose.unwrap_or("") {
        return Err(Error::PurposeMismatch);
    }

    Ok(rails)
}

/// `Object#to_s` for the JSON values a purpose could hold.
pub(crate) fn ruby_to_s(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        // Arrays and hashes to_s as their #inspect, which no purpose we compare against looks like.
        Some(other) => format!("\u{0}{other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_truncates_to_milliseconds() {
        let t: Timestamp = "2026-01-01T12:00:00.123999Z".parse().unwrap();
        assert_eq!(iso8601_millis(t), "2026-01-01T12:00:00.123Z");
        let t: Timestamp = "2046-01-01T12:00:00Z".parse().unwrap();
        assert_eq!(iso8601_millis(t), "2046-01-01T12:00:00.000Z");
    }
}
