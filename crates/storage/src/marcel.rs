//! Marcel 1.1.0 content type identification (`Marcel::MimeType.for`), over tables dumped from the
//! reference bundle (`reference-tools/storage/dump_tables.rb` → `tables.rs`).

use crate::tables::{self, Match};

pub const BINARY: &str = "application/octet-stream";

/// `Marcel::MimeType.for(io, name:, declared_type:)`, as `ActiveStorage::Blob#extract_content_type`
/// calls it on upload.
pub fn identify(data: &[u8], name: Option<&str>, declared_type: Option<&str>) -> String {
    let filename_type = name.and_then(by_path);
    most_specific_type(&[by_magic(data), for_declared_type(declared_type), filename_type.map(str::to_string)])
}

/// `Marcel::MimeType.for(extension:)`.
pub fn for_extension(extension: &str) -> String {
    most_specific_type(&[by_extension(extension).map(str::to_string)])
}

/// `Marcel::Magic.by_extension`: case-insensitive, with or without the leading dot.
pub fn by_extension(extension: &str) -> Option<&'static str> {
    let ext = extension.to_lowercase();
    let ext = ext.strip_prefix('.').unwrap_or(&ext);
    tables::EXTENSIONS.binary_search_by(|(e, _)| (*e).cmp(ext)).ok().map(|i| tables::EXTENSIONS[i].1)
}

/// `Marcel::Magic.by_path`: the extension per Ruby's `File.extname`.
pub fn by_path(path: &str) -> Option<&'static str> {
    by_extension(crate::filename::extname(path))
}

/// `Marcel::Magic.new(type).extensions`.
pub fn extensions(content_type: &str) -> &'static [&'static str] {
    tables::TYPE_EXTS.binary_search_by(|(t, _)| (*t).cmp(content_type)).map(|i| tables::TYPE_EXTS[i].1).unwrap_or(&[])
}

/// `Marcel::Magic.child?`.
pub fn is_child(child: &str, parent: &str) -> bool {
    child == parent || parents(child).iter().any(|p| is_child(p, parent))
}

fn parents(content_type: &str) -> &'static [&'static str] {
    tables::TYPE_PARENTS.binary_search_by(|(t, _)| (*t).cmp(content_type)).map(|i| tables::TYPE_PARENTS[i].1).unwrap_or(&[])
}

/// How many leading bytes [`by_magic`] can look at: identifying the first `magic_prefix_len()`
/// bytes of a file gives the same answer as identifying all of it.
pub fn magic_prefix_len() -> usize {
    fn reach(matches: &[Match]) -> usize {
        matches
            .iter()
            .map(|m| {
                let value = m.value.map_or(0, <[u8]>::len);
                let own = match m.range_end {
                    Some(end) => end + value,
                    None => m.offset + value,
                };
                own.max(reach(m.children))
            })
            .max()
            .unwrap_or(0)
    }
    static LEN: std::sync::LazyLock<usize> = std::sync::LazyLock::new(|| tables::MAGIC.iter().map(|(_, m)| reach(m)).max().unwrap_or(0));
    *LEN
}

/// `Marcel::Magic.by_magic`: the first table entry whose matches hit.
pub fn by_magic(data: &[u8]) -> Option<String> {
    tables::MAGIC.iter().find(|(_, matches)| matches_any(data, matches)).map(|(content_type, _)| content_type.to_lowercase())
}

fn matches_any(data: &[u8], matches: &[Match]) -> bool {
    matches.iter().any(|m| {
        let Some(value) = m.value else { return false };
        let hit = match m.range_end {
            // `io.read(offset.begin); io.read(offset.end - offset.begin + value.bytesize).include?(value)`
            Some(end) => read(data, m.offset, end - m.offset + value.len())
                .is_some_and(|window| value.is_empty() || window.windows(value.len()).any(|w| w == value)),
            None => read(data, m.offset, value.len()).is_some_and(|bytes| bytes == value),
        };
        hit && (m.children.is_empty() || matches_any(data, m.children))
    })
}

/// `IO#read(length)` after skipping `offset` bytes: `nil` at EOF, otherwise up to `length` bytes.
fn read(data: &[u8], offset: usize, length: usize) -> Option<&[u8]> {
    if length == 0 {
        return Some(&[]);
    }
    if offset >= data.len() {
        return None;
    }
    Some(&data[offset..(offset + length).min(data.len())])
}

/// Declared types are downcased, stripped of parameters, and ignored when binary.
fn for_declared_type(declared_type: Option<&str>) -> Option<String> {
    let declared = declared_type?.to_lowercase();
    let media_type = declared.split([';', ',', ' ', '\t', '\n', '\r', '\u{b}', '\u{c}']).next()?;
    (media_type.contains('/') && media_type != BINARY).then(|| media_type.to_string())
}

/// `most_specific_type(*candidates, BINARY)`: later candidates only win when they are children
/// of the current pick.
fn most_specific_type(candidates: &[Option<String>]) -> String {
    let mut unique: Vec<String> = Vec::new();
    for candidate in candidates.iter().flatten().cloned().chain([BINARY.to_string()]) {
        if !unique.contains(&candidate) {
            unique.push(candidate);
        }
    }
    let mut pick = unique[0].clone();
    for candidate in &unique[1..] {
        if is_child(candidate, &pick) {
            pick = candidate.clone();
        }
    }
    pick
}
