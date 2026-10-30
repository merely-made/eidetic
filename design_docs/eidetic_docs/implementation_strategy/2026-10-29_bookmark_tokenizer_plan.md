# Bookmark tokenizer implementation plan

**Status (2026-10-29): in progress.**

**Goal:** replace the independent Netscape bookmark HTML lexer, attribute reader and character-reference decoder with html5ever's tokenizer while preserving the bookmark projection, source evidence and public API.

**Spec:** `mere/design_docs/mere_docs/research/2026-10-29_stack_ownership_audit/REPORT.md`, R03. Cross-stack sequence: `mere/design_docs/mere_docs/implementation_strategy/2026-10-29_stack_reduction_plan.md`, P2.

**Architecture:** use the tokenizer only, not a DOM/tree builder. A small sink collects decoded tags/attributes and character tokens for the existing bookmark reducer. Comments and tokenizer diagnostics do not become imported content. Remove the independent HTML syntax/attribute/entity algorithms and avoid decoding tokenizer output a second time. Folder, timestamp, notice, source-byte and URL normalization policy remain owned by the importer.

## Global constraints

- Source base: Eidetic `f3424a9c48aa886bc0c07655bf81c459b2bba6d7`.
- Work only on S-PC, `/home/markik/Code/worktrees/eidetic-stack-reduction-20261029`, branch `stack-reduction-bookmarks-20261029`; preserve unrelated source/worktrees and pins.
- Working toolchain Rust 1.99.1; declared MSRV 1.97.1 remains unchanged. Wasm/MSRV and downstream receiving are separate gates.
- Build serially through `/home/markik/Code/testing/machine-heavy.lock`, `MemoryMax=3G`, `MemorySwapMax=0`, `CPUQuota=150%`, one Cargo job, one test thread, timeout 600 seconds, nice 10. Systemd needs explicit WorkingDirectory and Cargo PATH.
- Stable target: `/home/markik/Code/targets/lanes/stack-reduction-eidetic/build`; retained output: `/home/markik/Code/output/stack-reduction-bookmarks-20261029`.
- Add `html5ever = "0.39"` to the ordinary Eidetic library dependency set. It is already used by sibling receiving locks. Inspect the actual added dependency closure; do not upgrade unrelated packages or introduce a browser engine, DOM, GPU or network execution dependency.
- Preserve the signatures of `parse_netscape_bookmark_html` and `parse_netscape_bookmark_html_with_notices`, all public import output types and intake's exact source-byte retention.
- Preserve folder labels and positions, Unknown location, absent bookmark ID, timestamps, empty tags, URL normalization, and all six unrepresented-field notices. Character-reference decoding becomes standards-compliant and happens exactly once.
- Plain commits using command-local `git -c user.name=markik -c user.email=mark_ik@outlook.com`; no attribution trailers, pushes or shared-branch merges in implementation tasks.

## Review focus

1. Quoted `>` must remain inside the attribute; the complete HREF and independent timestamp/notice attributes must survive.
2. Full named character references must decode in text, folder labels and attributes; `&amp;copy;` becomes the literal `&copy;`, never a second-pass copyright character.
3. Comments containing fake anchors or folder headings must produce no bookmark or folder.
4. Standards tokenization must preserve source bytes as evidence and the surrounding folder/date/notice mapping. No HTML execution or DOM repair is implied.
5. No-default-feature import parsing must remain available; the new mandatory dependency closure must remain independently inspectable and pure Rust.

### Task 1: replace HTML syntax mechanics and qualify bookmark import

**Files:** `crates/eidetic-core/Cargo.toml`, `Cargo.lock`, `crates/eidetic-core/src/import/parser.rs`, and `crates/eidetic-core/src/import/tests.rs`.

**Interfaces:** preserve the existing two Netscape parsing functions and outputs. The internal tag representation may change from raw tag strings to decoded attributes. Keep the current token-order reducer and URL/title projection; do not add implied DOM nesting repair.

- [ ] Establish the unchanged focused importer baseline under the cap. Keep the full command and result. An unrelated dependency fetch/build failure is not a parser test failure and must remain explicit.
- [ ] Add independent regression cases through existing import APIs: quoted `>` in HREF and another attribute, full named entities in a folder/title/HREF, nested escaped `&amp;copy;`, and a comment such as `<!-- ignored > <A HREF="https://fake.test/">Fake</A><H3>Fake folder</H3> -->` followed by a real bookmark. Assert projected values and notices, not only counts. Include mixed-case markup and a valid anchor after irrelevant malformed markup without requiring browser tree repair.
- [ ] Run those focused new cases on the old parser and retain the demonstrated failures. Keep existing nested-folder, timestamp, source-preservation, missing/empty-HREF and unrepresented-field tests.
- [ ] Add html5ever and use its tokenizer API for the syntax work. Inspect its exact 0.39 source for sink/state APIs rather than assuming latest documentation matches. Store/concatenate decoded token data; skip comment/doctype/error tokens as appropriate. Remove the local raw-tag attribute parser and entity table/decoder. Preserve partial-input behavior deliberately and document any unavoidable standards correction.
- [ ] Resolve only the added tokenizer dependency closure, compare every package identity/dependency edge before and after, and forbid unrelated Git revision changes. Use `--locked` for subsequent tests.
- [ ] Run focused importer tests with `--no-default-features` and with package defaults; run the complete Eidetic library/integration suite without defaults once. Verify default library compilation as part of its focused importer run. Use `cargo +1.99.1 test --locked -j 1 -p eidetic --no-default-features --lib import::tests -- --test-threads=1`, corresponding default-feature focus, and `cargo +1.99.1 test --locked -j 1 -p eidetic --no-default-features -- --test-threads=1`.
- [ ] Check formatting of changed files and source diff whitespace. Preserve unrelated existing warnings/formatting. Record actual removed code, new package closure, test commands/counts, failures and unqualified targets. Commit only the Task 1 source/test/manifest/lock changes.
- [ ] Independent review checks spec compliance and code quality against the exact diff and evidence; resolve Important/Critical findings before marking this supplier cut complete.

## Findings

- 2026-10-29: the existing lexer ends tags at the first `>` without quote state; attributes and a small entity table are independently parsed. Import projection retains source bytes and notices and does not expose a DOM API.
- 2026-10-29: the existing package's dev dependencies include transport/identity test support. A focused library command still resolves a wider development graph; its actual cost and any source-fetch failure must be recorded rather than described as a tokenizer-only build.
- 2026-10-29: import is available without `pack-signing` or `json-schema`; the tokenizer therefore must not be hidden behind either feature.

## Progress

- 2026-10-29: current-main isolated S-PC lane founded; read-only preflight complete; unchanged baseline running.

## Deferred work

The supplier cut does not update Mere/Turnstone pins or claim browser, MSRV or installed-product qualification. Other HTML summary/feed parsers are separate source owners. Any richer malformed-tree repair is a distinct behavior decision rather than part of this tokenizer substitution.
