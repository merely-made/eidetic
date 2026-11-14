# house

The family's git-hook tooling and repository checks as one command, so a repository's
`.pre-commit-config.yaml` names one tool and nothing is copied between
repositories.

```text
house line-ceiling [--config PATH] FILE...
house decisions check | regen [--stage] | lookup ... | anchors ...
house resolved record | push | show COMMIT [--write [--force]]
house hooks [--no-decisions] [--bans DIR]... [--patches] [--clippy ARGS]... | house hooks --check
house cycles [--family-dir DIR | --remote] [--repo NAME]... [--allowlist PATH] [--dev-warn] [--strict-locks] [--verbose] [--json]
house doc-audit [--json] [--root DIR] [--docs DIR] [--ledger DIR] [--snapshot PATH] [--corrections PATH] [--moved-out PATH] [--receipt-relocations PATH] [--workspace DIR] [--snapshot-sha256 HEX]
house patches --from URL [--branch BRANCH] [--forks PREFIX] [--manifest PATH]... [--family-dir DIR | --remote] | house patches --check [--family-dir DIR | --remote]
```

## line-ceiling and decisions

`house line-ceiling` is the [line-ceiling](../line-ceiling) binary and
`house decisions` the [responsa](../responsa) binary, called through their
library entry points (`line_ceiling::run`, `responsa::cli::run_args`): same
arguments, output and exit codes. Both standalone binaries stay.

## resolved

No `Cargo.lock` is tracked. What cargo resolved for a commit is kept as a git
note on that commit under `refs/notes/resolved`, so an old build can be
rebuilt. This is a port of the `record-resolved.sh` script the repositories
carried.

- `record` (post-commit): at the repository top level, for every tracked
  `Cargo.toml` with a `Cargo.lock` beside it on disk, in path order, appends
  `# ---- <path>` and the lock's bytes, then attaches the text to `HEAD`,
  replacing any note there. It always exits 0, printing one line to stderr
  when the note could not be attached.
- `push` (pre-push): nothing when `RESOLVED_NOTES_PUSH` is set. Records first
  when `HEAD` has no note (a rebase leaves notes behind), fetches the remote's
  notes into `refs/notes/resolved-remote`, merges them with `cat_sort_uniq`,
  and pushes the notes ref with `RESOLVED_NOTES_PUSH=1` so the hook does not
  recurse. The remote is `PRE_COMMIT_REMOTE_NAME`, else `origin`. It never
  exits non-zero: a failure here must not block a branch push. A failed
  fetch, merge or push prints one line to stderr naming the ref, the remote
  and git's first error line; a remote with no notes yet stays quiet.
- `show COMMIT` prints the note. With `--write` it splits the note on its
  headers and writes each lock back to its path, refusing (and writing
  nothing) when any of them exists unless `--force`, and refusing any path
  that would leave the repository. Then build with `--locked`.

Fetching the notes in a fresh clone:
`git fetch origin refs/notes/resolved:refs/notes/resolved`.

## hooks

`house hooks` prints the canonical hook file, rendered from
[`src/template.yaml`](src/template.yaml): line ceiling, decisions check,
`cargo clippy --all-targets`, and the two resolved hooks, with
`fail_fast: true` and the pre-commit, post-commit and pre-push hook types.
`default_stages: [pre-commit]` keeps every hook but the two resolved ones at
pre-commit: a hook with no stages would otherwise run at every installed
stage, and the notes push inside `resolved push` fires pre-push again, where
a stale line-ceiling failure once kept the notes from reaching a remote.

- `--no-decisions` drops the decisions check, for repositories with no
  `design_docs/decisions.toml`.
- `--bans DIR`, repeatable, adds a duplicate-version pass over the workspace
  at `DIR` (`.` for the repository root): `cargo deny --workspace
  --all-features check -D unmatched-skip bans`, read against `DIR/deny.toml`.
  Name the root and every nested workspace that ships. Each workspace keeps
  its own skips, so a skip that matches nothing is stale and fails the commit.
- `--deny` adds `cargo deny check bans`: the root with default features only.
  `--bans .` replaces it; `--deny` and `--deny-args` leave when no repository
  names them, and cannot be combined with `--bans`.
- `--clippy ARGS`, repeatable, adds a `cargo clippy ARGS` pass per use (for
  a feature the workspace build leaves off).
- `--patches` adds `house patches --check` (see below), which checks every
  tracked manifest carrying a `# house patches` block.

The file's first line records the command that wrote it, for example
`# house hooks --deny --clippy '--all-targets -p mere-rdf --features mere-rdf/query'`.
`house hooks --check` reads that line from `.pre-commit-config.yaml` in the
current directory, renders the same options, and exits 1 with one line when
the file differs. A repository refreshes its file with
`house hooks <its options> > .pre-commit-config.yaml`.

Options: `--no-decisions`, `--regen` (run `house decisions regen --stage`
before the check), `--bans DIR` for each workspace, `--deny`, `--deny-args ARGS` (arguments between `cargo deny`
and `check bans`), `--clippy-base ARGS` (the first clippy pass, when
it is not `--all-targets`), `--clippy ARGS` for each further pass, and
`--local FILE` for a repository's own hooks: a YAML list at column 0, kept in
the repository and appended after the fixed hooks. The first line of the
generated file records the options, so `--check` needs none.

In the template a `#{name` line opens a block kept only when its option is
on (`clippy` and `bans` repeat once per pass), `#}` closes it, and `{{key}}` is
filled in.

The hooks call `house` from the PATH (`language: system`); install it once per
machine with `cargo install --git https://github.com/merely-made/eidetic house`
and rerun that to update. No hook builds a private copy, and no tag is named.

## cycles

`house cycles` is a port of Mere's `scripts/check_family_cycles.py`: same
options, report and exit codes. It reads the checkouts under
`--family-dir` (default: the directory holding the current repository) or,
with `--remote`, shallow clones of every `merely-made` repository; builds the
workspace-root edge graph from every family `git` row; leaves out what
`support/family-cycles/allowed.toml` rules; and fails on any normal, build or
(unless `--dev-warn`) dev cycle that remains, or on an allowlist entry that
sits on no cycle.

The per-lock check (a cycle inside one resolved graph warns, and fails under
`--strict-locks`) takes each lock from the `refs/notes/resolved` note on the
checkout's `HEAD`, else from a `Cargo.lock` on disk. `--remote` fetches the
notes beside each clone. An expected lock root (a `[workspace]` directory, or
a package under none) with neither is printed as
`no resolved versions recorded: <repo>/<path>` and the check goes on with the
manifest edges; `--json` lists them under `unresolved_roots`. Unlike the
script, the walk does not follow directory links or junctions.

## doc-audit

`house doc-audit` is a port of Mere's `scripts/mere_doc_judgment_audit.py`:
same report, `--json` and exit codes. It walks `design_docs/` (skipping
`archive_docs`), reads the D2 judgment records in
`support/doc-audit/d2/batch_*.md` and the legacy snapshot beside them, and
reports each active document lacking a record and each record for a document
no longer active. The root is the current repository (`--root` to change
it); the other paths default to the script's and resolve under the root. An
absent snapshot, corrections file or moved-out list counts as empty, so the
audit runs in a repository that has only the two directories.

## patches

Cargo applies `[patch]` only in the workspace being built, so a fork an
upstream (Mere) takes in its `[patch.crates-io]` does not follow it into the
repositories that depend on it: each of them resolves the unpatched crate and
carries two versions of whatever the fork moved. `house patches`, run in a
consumer, writes the upstream's fork rows into the consumer's own
`[patch.crates-io]`, and `--check` fails when they drift. The owner's ruling
(2026-11-14), on how the Oxigraph fork reaches Mere's consumers: "As D159, but
the rows are generated: `house` reads Mere's patch table and writes the
matching rows into each consumer, with a check that fails when they drift. No
hand-copied rows; one more thing `house` does."

- **The upstream** is named by its git URL, as the consumer's own dependency
  rows spell it: `--from https://github.com/merely-made/mere.git`. Its root
  `Cargo.toml` is read at `origin/<branch>` (`--branch`, default `main`) in
  the checkout under the family directory whose `origin` is that URL (https,
  ssh and scp forms and a trailing `.git` compare equal). The family
  directory is `--family-dir`, else the directory holding the current
  repository's main checkout: the same siblings `house cycles` reads, also
  from a linked worktree. Nothing is fetched: fetch the upstream first.
  `--remote` reads a shallow clone of the URL instead.
- **The rows carried** are the upstream's `[patch.crates-io]` rows whose
  `git` URL starts with `--forks PREFIX`, default `https://github.com/mark-ik/`,
  where the owner's patch-only forks live (one repository per crate). Path
  rows and family rows (`eidetic`, `muniment` in Mere's table) are the
  upstream's own business and are not carried. A carried row must be strings
  only and must not name `rev` or `tag`: rows follow branches. Its fields are
  copied in the upstream's order.
- **The rows kept** are those whose crate is in the consumer's graph. The
  workspace is resolved with `cargo metadata --all-features` (every target,
  as the lock's resolve), from the manifest's directory, with every carried
  row applied through `--config` (which wins over the manifest's own rows);
  a row is kept when its crate resolved from the row's URL. A row for a crate
  not in the graph would make cargo warn that the patch was not used, so it
  is dropped. Resolving can fetch the fork and the registry index; it writes
  the untracked `Cargo.lock` as any cargo command does.
- **The block** sits at the end of the manifest's `[patch.crates-io]`
  (before any comment heading the next table), or in a new table at the end
  of the file, and is rewritten in place on the next run:

  ```toml
  # house patches --from https://github.com/merely-made/mere.git
  # Written by `house patches` (eidetic, crates/house) from the upstream's [patch.crates-io]; `house patches --check` fails when they drift.
  oxrdf = { git = "https://github.com/mark-ik/oxigraph", branch = "mark-ik/0.5" }
  # end house patches
  ```

  Rows are sorted by crate. The first line records `--from`, and `--branch`
  and `--forks` when they are not the defaults. Every byte outside the block
  is kept, and the block takes the file's line ending. A hand-written row
  outside the block for a carried crate is refused, naming it: remove it, or
  the two would compete. `--manifest PATH`, repeatable, names each workspace
  root that needs rows (default `Cargo.toml`), for example Turnstone's root
  and `ports/graphshell/web/Cargo.toml`.
- **`--check`** finds every tracked `Cargo.toml` carrying a block, reads each
  block's first line for its options, recomputes the rows against the
  upstream's current table and the consumer's current graph, and prints one
  line per manifest that differs (rows missing, extra or changed, or the
  block edited), exiting 1. It takes no row options; `--family-dir` and
  `--remote` only say where the upstream is. With no block anywhere it fails.
- **No upstream commit is recorded.** The block follows the upstream's
  branch as every family row does; a commit in it would go stale with every
  upstream commit and fail the check for nothing. `--check` compares with the
  table at the fetched `origin/<branch>`, so it is as fresh as the last fetch.
