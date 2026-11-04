# Source splits: H6 Eidetic receipts

## 2026-11-03: fifteen owned source paths

Owner: house-style H6 / 01a19f86-cb71-7820-a6ac-aaec39d8bfd4.
Baseline: `375713f972310d47b57d1eac288c2fbf3350229c`. Compact text only, under H6's shared 100 MB
retained cap. Earlier eight and H5's fifty paths are excluded.

| Original source | Before | After |
|---|---:|---:|
| `crates/chartulary/src/commit.rs` | 1115 | 392 |
| `crates/chartulary/src/spine.rs` | 1494 | 389 |
| `crates/eidetic-core/src/browsing/page.rs` | 605 | 403 |
| `crates/eidetic-core/src/models/answer.rs` | 622 | 388 |
| `crates/eidetic-search/examples/eidetic-recall.rs` | 711 | 577 |
| `crates/eidetic-search/src/index.rs` | 667 | 376 |
| `crates/eidetic-search/tests/lexical_ngram_recall.rs` | 750 | 489 |
| `crates/luggage/src/install.rs` | 624 | 518 |
| `crates/luggage/src/updater.rs` | 701 | 452 |
| `crates/muniment/src/directory_backend.rs` | 1306 | 508 |
| `crates/servitor/src/delegation.rs` | 745 | 368 |
| `crates/servitor/src/gate.rs` | 1124 | 407 |
| `crates/servitor/src/run.rs` | 606 | 225 |
| `crates/stickleback/examples/inventory_compare/model.rs` | 615 | 547 |
| `crates/stickleback/src/drop.rs` | 818 | 592 |

Every new module also fits 600 physical lines. Fifteen exact exceptions
retired; five remain. Imported patches, static fixtures, recorded data,
file formats and dependency pins/lockfile remain unchanged. Public facade
paths stay intact; private impl helpers remain visible to their former owner.
The existing issuer-extension documentation link now names its actual trait.

- Baseline/post `cargo test --workspace --locked --no-fail-fast`: 746 passed
  test/doctest cases each. Existing ignored cases remain unchanged.
- `cargo test --workspace --locked --no-run`: pass after extraction.
- `cargo check -p eidetic-search --features recall-example --example eidetic-recall --all-targets --locked`:
  pass. This type-checks the optional model-backed example and its fixtures;
  it does not execute inference, fetch a model or qualify learned behavior.
- `cargo doc --workspace --locked --no-deps`: pass, with existing unrelated
  rustdoc warnings. The owned delegation link is repaired in the same pass.
- `prek run --all-files`: ceiling and Clippy pass; actual commit hook runs
  the same gates. Standalone tooling workspaces are outside this source pass.
- Structural review compares 299 old/new function and data entities.
  Scope/include-path adjustments and rustfmt blocks/separators are accounted
  for; operation and literal tokens are unchanged. Source notices retained.
- Stable approved target `C:/t/cargo-targets/eidetic`, two jobs. Subsequent
  gates use cached locked inputs. No isolated target, Cargo home, worktree,
  new live-session, hardware or other-platform acceptance is claimed.
