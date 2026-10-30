# line-ceiling: the shared source-size check and eidetic's hooks

**Date:** 2026-10-30

**Status, 2026-10-30:** plan, ruled (D1 to D3); nothing built. Opened by mere's house-style plan
(`mere/design_docs/mere_docs/implementation_strategy/2026-10-30_house_style_plan.md`),
whose phase H1 this carries for its first repository.

**Scope:** a line-ceiling binary in eidetic, a `.pre-commit-hooks.yaml` at
eidetic's root exposing it and `responsa check`, and isocosm as the first
repository to run both through prek, locally and in CI. The Clippy lint set
(house-style D6) and `cargo deny` hooks are later phases of the house-style
plan, not this one.

## Rulings carried

From the house-style plan, in the words given:

- **mere:stack.7**, the ceiling: no tracked source file over 600 physical lines,
  comments and blanks included, without a listed technical reason; generated
  and upstream sources excluded by declared path; one shared check at commit and
  in CI.
- **mere:house-style D7:** "Happy to use existing development systems that
  satisfy the requirements i've outlined, if possible."
- **mere:house-style D8:** "prek" runs the checks, from pre-commit's config
  format, locally and in CI.
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
  workflows; eidetic has `.github/workflows/libraries.yml`.
- 2026-10-30, prek's language reference: a `rust` hook repository is installed
  with `cargo install --bins --locked`, and virtual workspaces are supported.
  It does not say which packages a virtual-workspace install builds; E2
  measures that. prek stashes unstaged changes before running, so the files a
  hook receives at commit hold their staged content.

## Design

`line-ceiling [--config <path>] <files>...` reads the repository's
`line-ceiling.toml` and reports each named file over the ceiling that is
neither excluded nor excepted. prek passes the staged files at commit and every
tracked file under `prek run --all-files` in CI, so the binary reads files and
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
  `cargo test --workspace --locked` stays green in eidetic's CI.
- **E2. eidetic as a hook repository.** `.pre-commit-hooks.yaml` declares
  `line-ceiling` and `responsa-check` (`language: rust`). Done when prek
  installs both from a pinned eidetic rev on Windows and on a Mac or Linux
  machine, with the install time and the packages built recorded here; if a
  virtual-workspace install builds every eidetic binary, the measurement comes
  back as a checkpoint before E3.
- **E3. isocosm adopts.** `.pre-commit-config.yaml` pins eidetic's rev for both
  hooks; pack-json is ported to Rust in isocosm and runs as a hook (D3), its
  wiring (a local `language: rust` hook or a `system` hook on a workspace
  binary) chosen by what prek supports for a local Rust hook; `prek install`
  replaces `core.hooksPath`; a workflow runs `prek run --all-files`. Done when
  a planted 601-line file and a planted hand-written pack JSON each fail both
  the commit and the CI run, a clean tree passes both, and
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

## Progress

- 2026-10-30: plan written; forks ruled as D1 to D3.
