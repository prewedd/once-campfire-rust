//! `reference/app/models/ban.rb`

use std::net::IpAddr;

use rusqlite::{Connection, Row, params};

use crate::database::Tx;
use crate::error::{Errors, Result};
use crate::sql::{self, CachedStatements, query_all};
use crate::time::Timestamp;

#[derive(Debug, Clone, PartialEq)]
pub struct Ban {
    pub id: i64,
    pub user_id: i64,
    pub ip_address: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Ban {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            user_id: row.get("user_id")?,
            ip_address: row.get("ip_address")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    /// `Ban.banned?(ip_address)`
    pub fn banned(conn: &Connection, ip_address: &str) -> Result<bool> {
        sql::exists(conn, r#"SELECT 1 AS one FROM "bans" WHERE "bans"."ip_address" = ? LIMIT 1"#, [ip_address])
    }

    pub fn for_user(conn: &Connection, user_id: i64) -> Result<Vec<Self>> {
        query_all(conn, r#"SELECT * FROM "bans" WHERE "bans"."user_id" = ?"#, [user_id], Self::from_row)
    }

    /// `bans.create!(ip_address:)`, validating the address is public.
    pub fn create(tx: &Tx<'_>, user_id: i64, ip_address: &str) -> Result<Self> {
        Self::validate(ip_address).into_result()?;
        let now = tx.now();
        let id: i64 = tx.conn().query_row_cached(
            r#"INSERT INTO "bans" ("created_at", "ip_address", "updated_at", "user_id") VALUES (?, ?, ?, ?) RETURNING "id""#,
            params![now, ip_address, now, user_id],
            |r| r.get(0),
        )?;
        Ok(Self { id, user_id, ip_address: ip_address.into(), created_at: now, updated_at: now })
    }

    /// `ip_address_is_public`
    pub fn validate(ip_address: &str) -> Errors {
        let mut errors = Errors::default();
        match parse_ipaddr(ip_address) {
            Some(ip) if is_loopback(ip) || is_private(ip) || is_link_local(ip) => {
                errors.add("ip_address", "cannot be a private or internal IP address")
            }
            Some(_) => {}
            None => errors.add("ip_address", "is not a valid IP address"),
        }
        errors
    }
}

/// `IPAddr.new`: an address, optionally with a `/prefix` (masked), or an IPv6 in brackets.
fn parse_ipaddr(text: &str) -> Option<IpAddr> {
    let (address, prefix) = match text.split_once('/') {
        Some((address, prefix)) => (address, Some(prefix.parse::<u32>().ok()?)),
        None => (text, None),
    };
    let address = address.strip_prefix('[').and_then(|a| a.strip_suffix(']')).unwrap_or(address);
    let ip: IpAddr = address.parse().ok()?;
    Some(match (ip, prefix) {
        (IpAddr::V4(v4), Some(bits)) if bits <= 32 => {
            let mask = if bits == 0 { 0 } else { u32::MAX << (32 - bits) };
            IpAddr::V4((u32::from(v4) & mask).into())
        }
        (IpAddr::V6(v6), Some(bits)) if bits <= 128 => {
            let mask = if bits == 0 { 0 } else { u128::MAX << (128 - bits) };
            IpAddr::V6((u128::from(v6) & mask).into())
        }
        (_, Some(_)) => return None,
        (ip, None) => ip,
    })
}

// Ruby's IPAddr predicates, including their IPv4-mapped IPv6 handling, which only
// checks the `ffff` bits (`@addr & 0xffff_0000_0000 == 0xffff_0000_0000`).

fn mapped_v4(ip: IpAddr) -> Option<u32> {
    match ip {
        IpAddr::V6(v6) => {
            let addr = u128::from(v6);
            (addr & 0xffff_0000_0000 == 0xffff_0000_0000).then_some(addr as u32)
        }
        IpAddr::V4(_) => None,
    }
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => u32::from(v4) & 0xff00_0000 == 0x7f00_0000,
        IpAddr::V6(v6) => u128::from(v6) == 1 || mapped_v4(ip).is_some_and(|a| a & 0xff00_0000 == 0x7f00_0000),
    }
}

fn private_v4(a: u32) -> bool {
    a & 0xff00_0000 == 0x0a00_0000 || a & 0xfff0_0000 == 0xac10_0000 || a & 0xffff_0000 == 0xc0a8_0000
}

fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => private_v4(u32::from(v4)),
        IpAddr::V6(v6) => u128::from(v6) >> 121 == 0xfc >> 1 || mapped_v4(ip).is_some_and(private_v4),
    }
}

fn is_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => u32::from(v4) & 0xffff_0000 == 0xa9fe_0000,
        IpAddr::V6(v6) => u128::from(v6) >> 118 == 0xfe80 >> 6 || mapped_v4(ip).is_some_and(|a| a & 0xffff_0000 == 0xa9fe_0000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(ip: &str) -> Vec<String> {
        Ban::validate(ip).0.into_iter().map(|(_, m)| m).collect()
    }

    #[test]
    fn public_addresses_are_valid() {
        assert!(messages("8.8.8.8").is_empty());
        assert!(messages("2001:4860:4860::8888").is_empty());
    }

    #[test]
    fn internal_addresses_are_rejected() {
        for ip in [
            "127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.1.1", "169.254.169.254", "::1", "fc00::1", "fd12::1", "fe80::1",
            "::ffff:10.0.0.1", "::ffff:127.0.0.1",
        ] {
            assert_eq!(messages(ip), ["cannot be a private or internal IP address"], "{ip}");
        }
    }

    #[test]
    fn garbage_is_invalid() {
        assert_eq!(messages("not an ip"), ["is not a valid IP address"]);
        assert_eq!(messages(""), ["is not a valid IP address"]);
    }
}
