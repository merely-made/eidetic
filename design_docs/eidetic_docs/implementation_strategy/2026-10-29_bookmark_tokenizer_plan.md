# Bookmark parser reuse candidate

**Status, 2026-10-29: on hold; plan only.** No parser, Cargo manifest, lockfile or dependency change has been made on this branch. html5ever has not been selected for adoption.

## Concrete finding and possible cut

The inspected Netscape importer ends a tag at the first `>` without quoted-attribute state and owns attribute parsing plus a small named-entity table. A tokenizer could remove these syntax obligations while keeping the existing folder/date/location mapping, exact source-byte evidence and six unrepresented-field notices. A bounded parser repair is also a comparison candidate.

html5ever is used elsewhere in the family but was not in Eidetic’s inspected dependency closure. Compare the complete added package/features and default/cold build, fetch, artifact/disk and update costs before choosing it. Preserve pure Rust, MSRV 1.97.1, import without default features, Wasm, bounded input behavior and the existing public outputs. Sibling use does not make this dependency free.

If a tokenizer is chosen, keep the token-order reducer and avoid adding DOM/tree repair. Check quoted delimiters, comments containing fake anchors, named references and single-pass decoding (`&amp;copy;` stays `&copy;`), nested folders, timestamps, missing/empty HREF, malformed input and notices through the import API. Preserve raw input as evidence.

## Source and stopped baseline

Branch `stack-reduction-bookmarks-20261029` began at `f3424a9c48aa886bc0c07655bf81c459b2bba6d7`; plan commit `1f82bc6`. Current origin/main was observed at `20c9793181723a40858487d9d14623c81837eb57`. Reconcile source before any resumed implementation.

The unchanged focused command was `cargo +1.99.1 test --locked -j 1 -p eidetic --no-default-features --lib import::tests -- --test-threads=1`. It was stopped while fetching existing development dependencies, before compilation or tests. That is neither a parser-test failure nor a measured html5ever build cost. The log remains at `/home/markik/Code/output/stack-reduction-bookmarks-20261029/baseline.log` on S-PC.

No further implementation is running. The branch is preserved for comparison; do not advance consumers or shared main. A resumed candidate needs a concrete benefit-versus-cost decision, then an ordinary focused implementation and relevant tests, rather than a mandatory per-helper plan/review hierarchy.

Remote worktree: `/home/markik/Code/worktrees/eidetic-stack-reduction-20261029`. Existing serialized build caps remain MemoryMax=3G, MemorySwapMax=0, CPUQuota=150%, one Cargo job/test thread and bounded commands. Keep the existing target/output lanes, authority and toolchains.
