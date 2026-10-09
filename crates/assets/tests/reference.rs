//! Golden tests against what the reference app produced (tests/reference/*, written by
//! script/revendor from `assets:precompile` and the real Rails helpers). Files in `overrides/`
//! deliberately differ from the reference, so only their digests and bytes are allowed to.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/reference/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn json_fixture(name: &str) -> Value {
    serde_json::from_str(&fixture(name)).unwrap()
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The logical paths in `overrides/`.
fn override_files() -> Vec<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("overrides");
    let mut files = Vec::new();
    collect_files(&dir, &mut files);
    files.into_iter().map(|file| file.strip_prefix(&dir).unwrap().to_string_lossy().into_owned()).collect()
}

/// Each overridden logical path, with the reference's digested path and ours.
fn overridden() -> BTreeMap<String, (String, String)> {
    let reference = json_fixture("manifest.json");
    let ours: BTreeMap<&str, &str> = campfire_assets::manifest().iter().map(|(l, d)| (*l, *d)).collect();
    override_files()
        .into_iter()
        .filter_map(|logical| {
            let theirs = reference[&logical]["digested_path"].as_str()?;
            let ours = ours[logical.as_str()];
            assert_ne!(theirs, ours, "{logical} is overridden but digests the same");
            Some((logical, (theirs.to_string(), ours.to_string())))
        })
        .collect()
}

/// Logical paths the overrides add, which the reference doesn't have at all.
fn added() -> Vec<String> {
    let reference = json_fixture("manifest.json");
    override_files().into_iter().filter(|logical| reference.get(logical).is_none()).collect()
}

fn collect_files(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

/// `text` with our digested paths for overridden files replaced by the reference's.
fn as_reference(text: &str) -> String {
    overridden().values().fold(text.to_string(), |text, (theirs, ours)| text.replace(ours.as_str(), theirs))
}

fn get(path: &str) -> campfire_assets::StaticResponse {
    campfire_assets::serve(&campfire_assets::StaticRequest {
        method: "GET",
        path,
        ..Default::default()
    })
    .unwrap_or_else(|| panic!("{path} isn't served"))
}

#[test]
fn manifest_matches_the_reference_precompile() {
    let reference: BTreeMap<String, String> = json_fixture("manifest.json")
        .as_object()
        .unwrap()
        .iter()
        .map(|(logical, entry)| {
            (
                logical.clone(),
                entry["digested_path"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let added = added();
    let ours: BTreeMap<String, String> = campfire_assets::manifest()
        .iter()
        .filter(|(l, _)| !added.iter().any(|a| a == l))
        .map(|(l, d)| (l.to_string(), as_reference(d)))
        .collect();

    let missing: Vec<_> = reference
        .iter()
        .filter(|(l, d)| ours.get(*l) != Some(d))
        .collect();
    let extra: Vec<_> = ours
        .keys()
        .filter(|l| !reference.contains_key(*l))
        .collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "differs from reference: {missing:?}, extra: {extra:?}"
    );

    let served: Value = serde_json::from_str(&as_reference(campfire_assets::manifest_json())).unwrap();
    let mut served = served.as_object().unwrap().clone();
    for logical in &added {
        served.remove(logical);
    }
    let mut reference_json = json_fixture("manifest.json").as_object().unwrap().clone();
    // An overridden file's integrity hash covers its own bytes.
    for logical in overridden().keys() {
        served[logical].as_object_mut().unwrap().remove("integrity");
        reference_json[logical].as_object_mut().unwrap().remove("integrity");
    }
    assert_eq!(served, reference_json);
}

#[test]
fn compiled_files_are_byte_identical_to_the_reference_precompile() {
    let reference = json_fixture("compiled_sha256.json");
    let overridden: Vec<String> = overridden().into_values().map(|(theirs, _)| theirs).collect();
    let mut mismatched = Vec::new();
    for (digested_path, expected) in reference.as_object().unwrap() {
        if overridden.contains(digested_path) {
            continue;
        }
        let response = get(&format!("/assets/{digested_path}"));
        if sha256(&response.body) != expected.as_str().unwrap() {
            mismatched.push(digested_path.clone());
        }
    }
    assert!(
        mismatched.is_empty(),
        "compiled output differs for {mismatched:?}"
    );
    assert_eq!(
        reference.as_object().unwrap().len() + added().len(),
        campfire_assets::manifest().len()
    );
}

/// The reference's stylesheets, as it links them, plus the ones the overrides add (threads.css,
/// plans/threads.md), each linked and preloaded the same way.
#[test]
fn stylesheet_link_tag_all_matches_the_reference() {
    let tags = campfire_assets::stylesheet_link_tag_all(&[("data-turbo-track", "reload")]);
    let added: Vec<String> = added()
        .into_iter()
        .filter(|logical| logical.ends_with(".css"))
        .map(|logical| campfire_assets::stylesheet_path(&logical))
        .collect();
    assert!(!added.is_empty());
    let is_added = |href: &str| added.iter().any(|path| href.contains(&format!("\"{path}\"")) || href.contains(&format!("<{path}>")));

    let html: Vec<&str> = tags.html.lines().filter(|line| !is_added(line)).collect();
    assert_eq!(html.len() + added.len(), tags.html.lines().count(), "every added stylesheet is linked");
    assert_eq!(as_reference(&html.join("\n")), fixture("stylesheet_link_tag_all.html"));

    let header = campfire_assets::append_preload_links("", &tags.preload_links);
    let preloads: Vec<&str> = header.split(',').filter(|link| !is_added(link)).collect();
    assert_eq!(
        as_reference(&preloads.join(",")),
        fixture("link_header.txt")
    );
}

#[test]
fn javascript_importmap_tags_match_the_reference() {
    // The reference's pins, as it pins them, plus the modules the overrides add (the workspace
    // redesign), each pinned and preloaded like its neighbours.
    let added: Vec<String> = added()
        .into_iter()
        .filter(|logical| logical.ends_with(".js"))
        .map(|logical| campfire_assets::javascript_path(&logical))
        .collect();
    assert!(!added.is_empty());
    let tags = campfire_assets::javascript_importmap_tags();
    let lines: Vec<&str> = tags.lines().filter(|line| !added.iter().any(|path| line.contains(&format!("\"{path}\"")))).collect();
    assert_eq!(lines.len() + 2 * added.len(), tags.lines().count(), "every added module is pinned and preloaded");
    assert_eq!(as_reference(&lines.join("\n")), fixture("javascript_importmap_tags.html"));
}

#[test]
fn public_files_are_served_like_action_dispatch_static() {
    let overridden = overridden();
    for case in json_fixture("static_responses.json").as_array().unwrap() {
        let env = &case["env"];
        let path = case["path"].as_str().unwrap();
        let override_of = overridden.values().find(|(theirs, _)| path == format!("/assets/{theirs}"));
        let our_path = override_of.map(|(_, ours)| format!("/assets/{ours}"));
        let request = campfire_assets::StaticRequest {
            method: case["method"].as_str().unwrap(),
            path: our_path.as_deref().unwrap_or(path),
            range: env["HTTP_RANGE"].as_str(),
            accept_encoding: env["HTTP_ACCEPT_ENCODING"].as_str(),
            if_modified_since: None,
        };
        let label = format!("{} {} {env}", request.method, request.path);
        let expected_status = case["status"].as_u64().unwrap();
        let expected_headers = case["headers"].as_object().unwrap();

        // The probe's fallthrough app answers 404 with x-cascade: pass.
        if expected_status == 404 && expected_headers.get("x-cascade").is_some() {
            assert!(
                campfire_assets::serve(&request).is_none(),
                "{label} should fall through"
            );
            continue;
        }

        let response =
            campfire_assets::serve(&request).unwrap_or_else(|| panic!("{label} not served"));
        assert_eq!(response.status as u64, expected_status, "{label}");

        let ours: BTreeMap<String, String> = response
            .headers
            .iter()
            .filter(|(name, _)| *name != "last-modified")
            .map(|(name, value)| (name.to_string(), value.clone()))
            .collect();
        let theirs: BTreeMap<String, String> = expected_headers
            .iter()
            .map(|(name, value)| (name.clone(), value.as_str().unwrap().to_string()))
            .collect();

        // Our manifest lists the same entries in load-path order rather than the build
        // machine's readdir order, so its length and bytes can't match; an overridden file's
        // length, ETag and bytes are its own.
        if request.path == "/assets/.manifest.json" || override_of.is_some() {
            assert_eq!(
                ours.get("content-type"),
                theirs.get("content-type"),
                "{label}"
            );
            continue;
        }

        assert_eq!(ours, theirs, "{label}");
        if request.method == "GET" {
            assert_eq!(
                sha256(&response.body),
                case["body_sha256"].as_str().unwrap(),
                "{label}"
            );
        }
    }
}

#[test]
fn last_modified_round_trips_to_a_304() {
    let response = get("/robots.txt");
    let last_modified = response.header("last-modified").unwrap().to_string();
    let not_modified = campfire_assets::serve(&campfire_assets::StaticRequest {
        method: "GET",
        path: "/robots.txt",
        if_modified_since: Some(&last_modified),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(not_modified.status, 304);
    assert!(not_modified.headers.is_empty() && not_modified.body.is_empty());
}

#[test]
fn head_requests_have_no_body() {
    let response = campfire_assets::serve(&campfire_assets::StaticRequest {
        method: "HEAD",
        path: "/robots.txt",
        ..Default::default()
    })
    .unwrap();
    assert_eq!(response.status, 200);
    assert!(response.body.is_empty());
    assert_eq!(response.header("content-length"), Some("99"));
}

#[test]
fn multiple_ranges_are_multipart() {
    let sound = campfire_assets::audio_path("56k.mp3");
    let response = campfire_assets::serve(&campfire_assets::StaticRequest {
        method: "GET",
        path: &sound,
        range: Some("bytes=0-1, 4-5"),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(response.status, 206);
    // Rack sets multipart/byteranges, then Static overwrites it with the file's type.
    assert_eq!(response.header("content-type"), Some("audio/mpeg"));
    let body = String::from_utf8_lossy(&response.body);
    assert!(
        body.starts_with("\r\n--AaB03x\r\ncontent-type: audio/mpeg\r\ncontent-range: bytes 0-1/")
    );
    assert!(body.ends_with("\r\n--AaB03x--\r\n"));
}
