# house

The family's git-hook tooling as one command, so a repository's
`.pre-commit-config.yaml` names one tool and nothing is copied between
repositories.

```text
house line-ceiling [--config PATH] FILE...
house decisions check | regen [--stage] | lookup ... | anchors ...
house resolved record | push | show COMMIT [--write [--force]]
house hooks [--no-decisions] [--deny] [--clippy ARGS]... | house hooks --check
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
- `--deny` adds `cargo deny check bans`.
- `--clippy ARGS`, repeatable, adds a `cargo clippy ARGS` pass per use (for
  a feature the workspace build leaves off).

The file's first line records the command that wrote it, for example
`# house hooks --deny --clippy '--all-targets -p mere-rdf --features mere-rdf/query'`.
`house hooks --check` reads that line from `.pre-commit-config.yaml` in the
current directory, renders the same options, and exits 1 with one line when
the file differs. A repository refreshes its file with
`house hooks <its options> > .pre-commit-config.yaml`.

In the template a `#{name` line opens a block kept only when its option is
on (`clippy` repeats once per extra pass), `#}` closes it, and `{{key}}` is
filled in. The hooks install `house` from eidetic's `house-v<version>` tag,
as the two tools before it installed from theirs.

## Not folded yet

Mere's `scripts/check_family_cycles.py` and
`scripts/mere_doc_judgment_audit.py` are the next candidates.
