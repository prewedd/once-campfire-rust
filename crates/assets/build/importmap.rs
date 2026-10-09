//! A port of importmap-rails 2.2.2's Importmap::Map (lib/importmap/map.rb) for the subset of the
//! config/importmap.rb DSL the reference uses: `pin` and `pin_all_from` with `to:`, `under:` and
//! `preload:`.

use std::fs;
use std::path::{Path, PathBuf};

pub struct Pin {
    pub name: String,
    pub path: String,
    pub preload: bool,
}

enum Entry {
    Pin {
        name: String,
        to: Option<String>,
        preload: bool,
    },
    Dir {
        dir: String,
        under: Option<String>,
        to: Option<String>,
        preload: bool,
    },
}

/// Importmap::Map#expanded_packages_and_directories: pins in insertion order, then every
/// directory expanded; a later entry for an existing name keeps its position (a Ruby Hash).
/// A directory's files include those `overrides` adds at its logical path (`app/javascript/x`
/// is `x`), as if they were in the reference: overrides shadow the reference's files by logical
/// path, so they're pinned like them.
pub fn expand(importmap_rb: &Path, rails_root: &Path, overrides: &Path) -> Vec<Pin> {
    let source = fs::read_to_string(importmap_rb).unwrap();
    let mut packages: Vec<Pin> = Vec::new();
    let mut directories: Vec<(String, Option<String>, Option<String>, bool)> = Vec::new();

    for entry in source.lines().filter_map(parse_line) {
        match entry {
            Entry::Pin { name, to, preload } => {
                let path = to.unwrap_or_else(|| format!("{name}.js"));
                insert(
                    &mut packages,
                    Pin {
                        name,
                        path,
                        preload,
                    },
                );
            }
            Entry::Dir {
                dir,
                under,
                to,
                preload,
            } => {
                directories.retain(|d| d.0 != dir);
                directories.push((dir, under, to, preload));
            }
        }
    }

    for (dir, under, to, preload) in directories {
        let root = rails_root.join(&dir);
        if !root.exists() {
            continue;
        }
        let mut filenames = relative_javascript_files(&root);
        if let Some(logical) = ["app/javascript/", "vendor/javascript/"].iter().find_map(|prefix| dir.strip_prefix(prefix)) {
            filenames.extend(relative_javascript_files(&overrides.join(logical)));
        }
        filenames.sort();
        filenames.dedup();
        for filename in filenames {
            let name = module_name_from(&filename, under.as_deref());
            let path = [to.as_deref().or(under.as_deref()), Some(filename.as_str())]
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("/");
            insert(
                &mut packages,
                Pin {
                    name,
                    path,
                    preload,
                },
            );
        }
    }

    packages
}

/// The JavaScript files under `root`, as paths relative to it (none when it doesn't exist).
fn relative_javascript_files(root: &Path) -> Vec<String> {
    if !root.exists() {
        return Vec::new();
    }
    let mut files = Vec::new();
    javascript_files_in_tree(root, &mut files);
    files.into_iter().map(|file| file.strip_prefix(root).unwrap().to_string_lossy().into_owned()).collect()
}

fn insert(packages: &mut Vec<Pin>, pin: Pin) {
    match packages.iter_mut().find(|p| p.name == pin.name) {
        Some(existing) => *existing = pin,
        None => packages.push(pin),
    }
}

/// `[under, filename.chomp(extname).remove(/(?:\/|^)index$/).presence].compact.join("/")`
fn module_name_from(filename: &str, under: Option<&str>) -> String {
    let extname = crate::propshaft::extname(filename);
    let stem = filename.strip_suffix(extname).unwrap_or(filename);
    let stem = if stem == "index" {
        ""
    } else {
        stem.strip_suffix("/index").unwrap_or(stem)
    };
    [under, Some(stem).filter(|s| !s.is_empty())]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("/")
}

/// `Dir[path.join("**/*.js{,m}")]`: Dir globs skip dotfiles and dot-directories.
fn javascript_files_in_tree(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            javascript_files_in_tree(&path, files);
        } else if name.ends_with(".js") || name.ends_with(".jsm") {
            files.push(path);
        }
    }
}

fn parse_line(line: &str) -> Option<Entry> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (command, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let mut args = Args {
        rest: rest.trim_start(),
    };
    let first = args
        .string()
        .unwrap_or_else(|| panic!("importmap.rb: expected a string in {line:?}"));
    let (mut to, mut under, mut preload) = (None, None, true);
    while let Some((key, value)) = args.option() {
        match (key.as_str(), value) {
            ("to", Value::Str(v)) => to = Some(v),
            ("under", Value::Str(v)) => under = Some(v),
            ("preload", Value::Bool(v)) => preload = v,
            (key, _) => panic!("importmap.rb: unsupported option {key:?} in {line:?}"),
        }
    }
    match command {
        "pin" => Some(Entry::Pin {
            name: first,
            to,
            preload,
        }),
        "pin_all_from" => Some(Entry::Dir {
            dir: first,
            under,
            to,
            preload,
        }),
        other => panic!("importmap.rb: unsupported statement {other:?}"),
    }
}

enum Value {
    Str(String),
    Bool(bool),
}

struct Args<'a> {
    rest: &'a str,
}

impl Args<'_> {
    fn string(&mut self) -> Option<String> {
        let quote = self
            .rest
            .chars()
            .next()
            .filter(|c| *c == '"' || *c == '\'')?;
        let end = self.rest[1..].find(quote)? + 1;
        let value = self.rest[1..end].to_string();
        self.rest = self.rest[end + 1..].trim_start();
        Some(value)
    }

    fn option(&mut self) -> Option<(String, Value)> {
        if self.rest.is_empty() || self.rest.starts_with('#') {
            return None;
        }
        let rest = self
            .rest
            .strip_prefix(',')
            .unwrap_or_else(|| panic!("importmap.rb: can't parse {:?}", self.rest));
        let (key, rest) = rest
            .trim_start()
            .split_once(':')
            .expect("importmap.rb: expected key: value");
        self.rest = rest.trim_start();
        let value = if let Some(s) = self.string() {
            Value::Str(s)
        } else if let Some(rest) = self.rest.strip_prefix("true") {
            self.rest = rest.trim_start();
            Value::Bool(true)
        } else if let Some(rest) = self.rest.strip_prefix("false") {
            self.rest = rest.trim_start();
            Value::Bool(false)
        } else {
            panic!("importmap.rb: unsupported value {:?}", self.rest)
        };
        Some((key.trim().to_string(), value))
    }
}
