# responsa: fading and the anchor key

**Date:** 2026-10-23

**Status, 2026-10-23:** plan; nothing built. Opened when isocosm's
decision-links plan was archived, to hold the two points it left owed to this
tool. responsa 0.0.1 (eidetic `aa94fe22`) does neither.

**Scope:** `crates/responsa` only. The rule it keeps is core §11 of each
repository's `DOC_POLICY.md`; this plan changes no rule, it builds what rule 5
already names and keeps one check honest.

## Why

Core §11 rule 5 says a statement "whose governed code is gone or whose standing
has faded enters that round", the consolidation round, "to be kept, merged or
retired". The tool measures the budget and, with an index, whether governed
code is gone. It has no notion of standing, and the rate and floor that would
define "faded" are set nowhere. So half of rule 5 is a sentence with no
mechanism and no numbers.

The anchor check reads rust-analyzer's SCIP index, and rust-analyzer's `scip`
subcommand is marked unstable. The decision-links plan ruled that the
answer-key test reruns on each toolchain bump, and wrote it nowhere a toolchain
bump would find it.

Origin and the rulings quoted below:
`isocosm/design_docs/archive_docs/2026-10-22/2026-10-20_decision_links_plan.md`,
Decisions ("Fading", "Anchors") and Findings.

## Direction

- **Fading marks candidates; the maintainer decides** (the plan's answer,
  "Fading marks candidates; you decide"). Nothing is retired by the tool.
- **Standing is renewed only by a citation** in a commit, a plan or a question
  round, never by a read. Renewal spreads along plans and code modules, not
  areas.
- **A statement that governs live code does not fade.** The anchor check says
  which code is live.
- **The rate and floor are the maintainer's to set** and are read from a
  repository's configuration, not compiled in.
- **The anchors answer: "rust-analyzer, run per round".** The check is only as
  good as the index, so the index is tested against a known key whenever the
  toolchain that produces it changes.

## Phases and done-conditions

### A1. The answer key reruns on each toolchain bump

The test already exists: `crates/responsa/examples/answer_key.rs`, run as
`cargo run --release --example answer_key -- <index.scip> <truth.json>`. It
replays the anchor probe's key (definitions from git history, judged present,
moved or removed) through the resolver against a SCIP index of isocosm at
`afbce8a0`. The key and the probe's report are in
`Code/testing/isometry/anchor-probe-2026-10-20/`.

Done when, for a toolchain a consumer has just moved to:

- the index is rebuilt with that toolchain's `rust-analyzer scip`;
- the example judges 2,479 of 2,480 right, as on 2026-10-21 (the one miss is a
  function and a module that share one path, `isocosm::history::command`,
  because anchors carry no kind);
- a different count is a finding, written up before the bump lands, and the
  Progress entry names the toolchain version either way.

Isocosm's pin is `1.99.1` (`rust-toolchain.toml`), the toolchain
rust-analyzer was installed for on 2026-10-20. The first rerun is the next
bump of any consumer's pin.

### F1. Fading marks candidates

Done when:

- `check` reports each statement that has faded as a candidate, with the
  reason, and retires nothing;
- the rate and floor come from repository configuration, so two repositories
  can differ;
- a test shows that a read does not renew standing, that a citation in a
  commit, a plan or a question round does, and that renewal runs along the
  plan or code module that cited, not across the area;
- a test shows that a statement with a present anchor is never a candidate;
- the report is read-only on the manifest and doctrine, as the budget report
  is.

F1 waits on question 1 only for the numbers. The mechanism can be built with
them read from configuration.

## Decisions still to make

1. **The fading rate and floor.** Precedents the decision-links plan found, to
   weigh: the sim's reach field fades a strength by `decay_per_tick` down to a
   `legend_floor`, and legends do not fade
   (`isocosm/shared/isocosm/src/reach.rs`); the hagiograph's feat rule judges
   a mark that stood before the batch
   (`crates/hagiograph/src/reckoning.rs`). The doctrine's size is evidence
   too: isocosm's holds 133 statements across 17 areas, within its budget. Put as a card round with those numbers; recorded as
   `responsa D1` when answered.

## Findings

- **2026-10-23, what the tool does today** (read, with a positive control, not
  run). 1,999 lines in `src/`. Subcommands (`src/cli.rs`): `regen [--stage]`,
  `check`, `lookup`, and `anchors (--index FILE.scip | --scip CRATE_DIR)`.
  `check` reports a stale manifest, budget overruns and links into sibling
  repositories that do not resolve (README). A search of `src/`, the README,
  `Cargo.toml` and `examples/` for fade, standing, renew and candidate finds
  nothing, while the same search for budget finds `budget.rs`, `cli.rs` and
  `lib.rs`.
- **2026-10-21, the anchor resolver** (carried from the decision-links plan).
  Replaying the probe's key against SCIP's index judges 2,479 of 2,480 right.
  SCIP sees enum variants and fields where sem does not, resolves aliases and
  macros, and costs about 120 s and 40 MB a run. A move changes a SCIP symbol,
  so a moved anchor is relocated by qualified name. The `scip` subcommand is
  unstable; its format is not.

## Progress

- **2026-10-23:** opened from the decision-links plan's archive. Nothing built;
  question 1 is with the maintainer.
