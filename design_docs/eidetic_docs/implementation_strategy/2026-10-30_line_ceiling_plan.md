# line-ceiling: the shared source-size check and eidetic's hooks

**Date:** 2026-10-30

**Status, 2026-10-30:** in progress. E1 landed; E2 ruled (D4 to D7), its
Windows install run next. Opened by mere's house-style plan
(`mere/design_docs/mere_docs/implementation_strategy/2026-10-30_house_style_plan.md`),
whose phase H1 this carries for its first repository.

**Scope:** a line-ceiling binary in eidetic, tagged releases of it and of
responsa that repositories install through prek (D4), and isocosm as the first
repository to run both through prek, locally (D6). The Clippy lint set
(house-style D6) and `cargo deny` hooks are later phases of the house-style
plan, not this one.

## Rulings carried

From the house-style plan, in the words given:

- **mere:stack.7**, the ceiling: no tracked source file over 600 physical lines,
  comments and blanks included, without a listed technical reason; generated
  and upstream sources excluded by declared path; one shared check at commit,
  and over the whole tree in each repository's local gate (D6).
- **mere:house-style D7:** "Happy to use existing development systems that
  satisfy the requirements i've outlined, if possible."
- **mere:house-style D8:** "prek" runs the checks, from pre-commit's config
  format (CI dropped by D6).
- **mere:house-style D9:** "eidetic, as a Rust binary": a separate binary
  beside responsa, exposed as a hook; repositories pin eidetic's rev.
- The workspace data-formats rule: a file a person authors is TOML.

## Findings

- 2026-10-30: isocosm's `scripts/line-ceiling.ps1` (47 lines) counts physical
  lines as .NET `ReadAllLines` does, checks staged blobs with `-Staged`, and
  reads `line-ceiling-exceptions.txt` (`path<TAB>reason`, zero entries today).
  Its `.githooks/pre-commit` also runs `scripts/pack-json.ps1` (isocosm ruling
  624) and passes without checking where no PowerShell exists.
- 2026-10-30: isocosm's `shared/responsa` is a 1-file runner (`responsa-run`)
  that pins eidetic's responsa at `aa94fe22` for its hook. A prek hook pinned
  to an eidetic rev replaces it.
- 2026-10-30: isocosm has a GitHub remote (`merely-made/isocosm`) and no
  workflows; eidetic's were removed too (`f1ada1a`, 2026-10-28: "Gates live in
  local scripts and dated receipts").
- 2026-10-30, prek's language reference: a `rust` hook repository is installed
  with `cargo install --bins --locked`, and virtual workspaces are supported.
  It does not say which packages a virtual-workspace install builds; E2
  measures that. prek stashes unstaged changes before running, so the files a
  hook receives at commit hold their staged content.

## Design

`line-ceiling [--config <path>] <files>...` reads the repository's
`line-ceiling.toml` and reports each named file over the ceiling that is
neither excluded nor excepted. prek passes the staged files at commit and every
tracked file under `prek run --all-files`, so the binary reads files and
never calls git.

```toml
# Illustrative, not final.
ceiling = 600
extensions = ["rs", "ps1", "py", "js", "ts", "wgsl", "sh"]

[[exclude]]            # generated or upstream, by declared path
path = "vendor/"
reason = "imported upstream sources"

[[exception]]          # one file over the ceiling, with its technical reason
path = "src/tables.rs"
reason = "a generated lookup table checked in for build speed"
```

An entry without a reason is a configuration error. A physical line is a
`\n`-terminated line, plus a final unterminated one; `\r\n` counts once.

## Phases

- **E1. The binary.** Done when its tests show: 600 lines pass and 601 fail;
  CRLF, a missing final newline and an empty file count as `ReadAllLines`
  would; an exclude or exception without a reason is refused; an excepted file
  passes; and a stale exception fails (D2).
  `cargo test --manifest-path crates/line-ceiling/Cargo.toml --locked` passes.
- **E2. Tagged tool releases (D4, D5, D7).** Both tools are standalone
  packages; eidetic tags `line-ceiling-v0.0.2` and `responsa-v0.0.2` (the
  v0.0.1 tags stay, pointing at the workspace-member builds); the line-ceiling
  README carries the hook stanza a repository copies. Done when prek installs
  both from their tags on Windows with install times recorded here, and a Mac
  or Linux run is recorded when one is reachable.
- **E3. isocosm adopts.** `.pre-commit-config.yaml` pins eidetic's rev for both
  hooks; pack-json is ported to Rust in isocosm and runs as a hook (D3), its
  wiring (a local `language: rust` hook or a `system` hook on a workspace
  binary) chosen by what prek supports for a local Rust hook; `prek install`
  replaces `core.hooksPath`; isocosm's local gate runs `prek run --all-files`
  (D6). Done when a planted 601-line file and a planted hand-written pack JSON
  each fail both the commit and the full local run, a clean tree passes both,
  and
  `scripts/line-ceiling.ps1`, `scripts/pack-json.ps1`, the exceptions file,
  `.githooks/` and `shared/responsa` are deleted.

## Decisions

- **D1. Package shape (Mark, 2026-10-30):** "Small crates/line-ceiling", over a
  second `[[bin]]` in responsa's package. Its own package with almost no
  dependencies (`toml`, `serde`), so a prek install builds little and source
  linting stays apart from the decisions tool.

  Links: draws on mere:house-style D9 · summary line-ceiling is its own small package in eidetic.

- **D2. A stale exception fails (Mark, 2026-10-30):** "Fail", over warning. An
  exception whose file is now under the ceiling or gone fails the check, so the
  commit that shrinks a file also removes its entry.

  Links: draws on mere:stack.7 · summary A stale exception fails the check; the inventory shrinks only deliberately.

- **D3. pack-json is ported (Mark, 2026-10-30):** "Port it to Rust in isocosm",
  over a local prek hook running the PowerShell script. Every check then runs
  without PowerShell.

  Links: summary isocosm's pack-json check is ported to Rust in isocosm and runs as a prek hook.

- **D4. Tagged eidetic releases (Mark, 2026-10-30):** "Tagged eidetic releases",
  over publishing to crates.io or a system hook with a pinned bootstrap install.
  eidetic pushes a tag per tool release (`line-ceiling-v0.0.1`,
  `responsa-v0.0.1`); each repository's prek config carries a local hook naming
  the tag and package (`cli:https://github.com/merely-made/eidetic:<tag>:<package>`).
  eidetic serves no `.pre-commit-hooks.yaml`, since prek cannot install from its
  workspace root. A tag, once pushed, is never moved.

  Links: amends line-ceiling E2 · summary Repositories install eidetic's tools by tag through prek's cli form; eidetic is not a hook repository.

- **D5. line-ceiling is its own workspace (Mark, 2026-10-30):** "Also 1", the
  standalone package, over staying a workspace member. `crates/line-ceiling`
  carries its own `[workspace]` and `Cargo.lock`, so a tagged install fetches
  only `serde` and `toml`, not eidetic's git sources (murm alone is about
  1.9 GB of inherited Servo history). Asked with it, for responsa: "Neither;
  rethink responsa", over git's command-line fetch on machines and CI.

  Links: amends line-ceiling D1 · summary line-ceiling is a standalone package so its install fetches no workspace git sources.

- **D6. Local only (Mark, 2026-10-30):** "Local only", over one prek-only
  workflow or CI as recorded. Found contradicting mere:stack.7 and
  mere:house-style D3/D8 after CI was removed from eidetic and isocosm on
  2026-10-28. The checks run in the prek pre-commit hook and, over the whole
  tree, as `prek run --all-files` in each repository's local gate; a bypassed
  hook is caught at the next full run.

  Links: amends mere:stack.7, mere:house-style D3, mere:house-style D8 · summary The shared checks run locally, at commit and in each repository's full local gate; no CI.

- **D7. responsa is its own workspace (Mark, 2026-10-30):** "Standalone, like
  line-ceiling", over keeping it a workspace member. It reaches impresa and
  muniment by path and locks 38 crates.io packages and no git sources; its 14
  tests pass under `cargo test --manifest-path crates/responsa/Cargo.toml
  --locked`.

  Links: draws on line-ceiling D5 · summary responsa is a standalone package so its install fetches no workspace git sources.

## Progress

- 2026-10-30: plan written; forks ruled as D1 to D3.
- 2026-10-30: **E1 landed.** `crates/line-ceiling` (lib, binary, 11 tests,
  Clippy clean), default extensions isocosm's set, a missing
  `line-ceiling.toml` meaning the defaults. Control over each repository's
  tracked files: isocosm 0 over (agrees with its own script), eidetic 20,
  mere 411 (the house-style count's 407 plus the html, css and other
  extensions it did not count).
- 2026-10-30: **E2 checkpoint.** prek 0.5.9 installed (`cargo install --locked`).
  As a hook repository, eidetic does not install: prek ran `cargo install --bins
  --path .` at the virtual-workspace root, without `--locked`, and cargo spent
  over ten minutes (589 s of its own CPU, no `rustc` started) before it was
  stopped. A package-selecting install, `cargo install --locked --git <eidetic>
  --rev <sha> line-ceiling`, builds and installs in 38 s. prek's own
  package-selecting form, `additional_dependencies: ["cli:<url>:<tag>:<package>"]`,
  passes the pin to cargo as `--tag`, so it pins by tag only: a commit sha
  fails with "failed to find tag". Ruled as D4; the unpushed
  `.pre-commit-hooks.yaml` commit was dropped.
- 2026-10-30: **E2, second checkpoint.** Tags `line-ceiling-v0.0.1` and
  `responsa-v0.0.1` pushed at `7971d14`. Through prek on isocosm, both
  installs (`cargo install --git … --tag … <package> --locked`) timed out at
  580 s, each still fetching git dependencies: a git install loads the whole
  eidetic workspace, so it fetches every member's git sources (p2panda,
  dramatis, murm) whatever the package needs. murm is about 1.9 GB on GitHub
  and cargo's built-in git fetch is slow on it. The same tagged install by
  hand with `CARGO_NET_GIT_FETCH_WITH_CLI=true` finished in 87 s, on a cache
  the earlier attempts had partly filled; a fresh CI machine would fetch murm
  whole.
