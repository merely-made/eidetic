// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn sample_run() -> BrowserImportRun {
    BrowserImportRun {
        import_id: "import-1".to_string(),
        source: BrowserImportSource {
            browser_family: BrowserFamily::Chrome,
            profile_hint: Some("Default".to_string()),
            source_kind: BrowserImportSourceKind::BookmarkFile,
            stable_source_id: Some("chrome:default".to_string()),
        },
        mode: BrowserImportMode::OneShotFile,
        observed_at_unix_secs: 1_744_329_600,
        user_visible_label: "Chrome bookmarks".to_string(),
    }
}

#[test]
fn detects_chrome_json_bookmark_file() {
    assert_eq!(
        detect_bookmark_file_format(" { \"roots\": {} } "),
        Some(BookmarkFileFormat::ChromeJson)
    );
}

#[test]
fn parses_chrome_json_bookmarks_into_folder_paths() {
    let json = r#"
        {
          "roots": {
            "bookmark_bar": {
              "children": [
                {
                  "type": "folder",
                  "name": "Rust",
                  "id": "10",
                  "children": [
                    {
                      "type": "url",
                      "id": "11",
                      "name": "Docs",
                      "url": "https://docs.rs"
                    }
                  ]
                }
              ],
              "name": "Bookmarks Bar",
              "type": "folder"
            }
          }
        }
        "#;

    let batch = parse_bookmark_file_to_batch(json, sample_run()).unwrap();
    assert_eq!(batch.items.len(), 1);
    let BrowserImportPayload::Bookmark(item) = &batch.items[0] else {
        panic!("expected bookmark payload");
    };
    assert_eq!(item.page.canonical_url, "https://docs.rs/");
    assert_eq!(item.location, BookmarkLocation::Toolbar);
    assert_eq!(item.folder_path.len(), 1);
    assert_eq!(item.folder_path[0].label, "Rust");
}

#[test]
fn parses_netscape_bookmarks_into_nested_folder_paths() {
    let html = r#"
        <!DOCTYPE NETSCAPE-Bookmark-file-1>
        <DL><p>
          <DT><H3>Research</H3>
          <DL><p>
            <DT><A HREF="https://example.com/article">Example Article</A>
          </DL><p>
        </DL><p>
        "#;

    let items = parse_bookmark_items(html).unwrap();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item.page.canonical_url, "https://example.com/article");
    assert_eq!(item.folder_path.len(), 1);
    assert_eq!(item.folder_path[0].label, "Research");
    assert_eq!(
        item.page.normalized_title.as_deref(),
        Some("Example Article")
    );
}

#[test]
fn intake_keeps_source_bytes_and_reports_projection_losses() {
    let json = r#"{
        "version": 1,
        "roots": {"bookmark_bar": {"children": [
            {"type":"url", "id":"kept", "name":"Docs", "url":"https://docs.rs", "guid":"opaque"},
            {"type":"url", "id":"missing", "name":"No URL"},
            {"type":"url", "id":"empty", "name":"Empty URL", "url":"  "}
        ]}}
    }"#;
    let intake =
        parse_bookmark_file_to_intake(json, sample_run(), Some("file:///profile/Bookmarks".into()))
            .unwrap();
    assert_eq!(intake.source.bytes, json.as_bytes());
    assert_eq!(
        intake.source.content_hash(),
        crate::Hash::of(json.as_bytes())
    );
    assert_eq!(
        intake.value,
        parse_bookmark_file_to_batch(json, sample_run()).unwrap()
    );
    assert_eq!(intake.value.items.len(), 1);
    assert_eq!(
        intake
            .notices
            .iter()
            .filter(|notice| notice.kind == IntakeNoticeKind::SkippedRecord)
            .count(),
        2,
    );
    assert!(intake.notices.iter().any(|notice| {
        notice.kind == IntakeNoticeKind::UnrepresentedField
            && notice.source_locator.as_deref() == Some("kept.guid")
    }));
    let round_trip: Intake<BrowserImportBatch> =
        serde_json::from_slice(&serde_json::to_vec(&intake).unwrap()).unwrap();
    assert_eq!(round_trip, intake);
}

#[test]
fn netscape_intake_reports_a_skipped_anchor_and_preserves_its_source() {
    let html = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
        <DL><DT><A>Missing address</A>
        <DT><A HREF="gemini://example.test/" TAGS="research">Gemini</A></DL>"#;
    let intake = parse_bookmark_file_to_intake(html, sample_run(), None).unwrap();
    assert_eq!(intake.value.items.len(), 1);
    assert_eq!(intake.source.bytes, html.as_bytes());
    assert!(
        intake
            .notices
            .iter()
            .any(|notice| notice.kind == IntakeNoticeKind::SkippedRecord)
    );
    assert!(intake.notices.iter().any(|notice| {
        notice.kind == IntakeNoticeKind::UnrepresentedField
            && notice.source_locator.as_deref() == Some("gemini://example.test/.tags")
    }));
}

#[test]
fn legacy_history_payload_without_dwell_deserializes_unchanged() {
    let json = r#"{
        "page":{"canonical_url":"https://example.test/","normalized_title":null,
          "raw_url":null,"raw_title":null,"favicon_url":null},
        "visit_id":"v1","visited_at_unix_secs":123,"visit_count_hint":null,
        "transition":"Link","referring_url":null,"session_context":null
    }"#;
    let item: ImportedHistoryVisitItem = serde_json::from_str(json).unwrap();
    assert_eq!(item.view_time_ms, None);
    assert_eq!(item.transition, Some(HistoryTransitionKind::Link));
}
