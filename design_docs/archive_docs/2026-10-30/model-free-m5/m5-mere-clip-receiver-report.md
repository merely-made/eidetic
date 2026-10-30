# M5 Mere clip receiver repair

The Mere worktree was clean at `cc028fb4a05736528d787dd49327c600159fc0ad` (`origin/main`) before these edits. No `AGENTS.md` was present in or above the Mere worktree.

`retained_clip::read_store` returns `eidetic::Result`, while Graphshell's retained clip APIs expose `Result<_, String>`. Added explicit error mapping at the import verification and `ContentHost::open_clip` boundaries, preserving errors and adding operation context. The browser's async clip-reading closure is also inferred as `Result<HeldContent, String>`; mapped that third production call for the same reason. No supplier API, global conversion, lockfile, or test was changed.

## Changed source hashes

- `ports/graphshell/src/retained_content.rs`: `272f6d56464fda22e0748c03ba38c72472cb20e453d89815afcf3bcf71e9e111`
- `ports/graphshell/src/web_tree/reading.rs`: `2877b1b778f23fe86fcee4f9000ac2ebb88714bbc4da5d8122547811dcc2b712`

## Validation

- `rustfmt --edition 2024 --emit stdout` parsed both changed files successfully.
- `git diff --check` passed.
- `rustfmt --check` was attempted, but reports existing formatting drift elsewhere in `retained_content.rs`; whole-file formatting was avoided to keep the patch narrow.
- Guarded compile check: `cargo check -p graphshell --lib --locked --offline` did not reach compilation. The refreshed Mere lock pins Genet `c0144fe5`, absent from the Cargo Git checkout; offline mode refused to fetch it. The gate made no file or lock changes. Receipt/log: `/home/markik/Code/testing/custody-contract-20261029/m5-mere-graphshell-1.{json,log}`.

This remains a source-level adapter repair; compilation is unverified due to the missing pinned dependency checkout. It does not establish browser runtime behavior, offline restart behavior, or broader M5 qualification.
