# Model-free retained comparison receipts, 2026-10-30

Canonical plan: [P4a M3](../../eidetic_docs/implementation_strategy/2026-10-11_eidetic_development_plan.md#m3-compare-exact-retained-versions).
Earlier M1/M2 evidence remains in the [October 29 archive](../2026-10-29/2026-10-11_eidetic_development_plan_receipts.md).

## Published source and scope

Upstream was fetched and the existing clean isolated worktrees were checked for
fast-forward updates before implementation. Eidetic started at `09f7537` and
Turnstone at `6646433538d1a71a01db6da17b30871952c67b96`; upstream did not advance
during this slice. Provider commits `5e05083` and `a90aa678d59abac1803b878fc57a16cef8957e60` are published.
Turnstone implementation is `8e8a1b4d` plus review correction `29fa240e2173cf50eb2f182fb6226071aa9fafc3`;
its documentation/publication commit is `cce607d38f5084a8b1ddec3361f7fd04e301b000`.

The provider compares two exact retained texts with deterministic line edits,
full version/hash identities and reconstructible Unicode-scalar spans. Input,
identity, comparison work and output limits are explicit. Turnstone selects an
explicit Left/Right pair from admitted records, shows both complete typed
identities, and opens each side or changed passage through its exact retained
reader. Source owners currently supply no authoritative head or lineage, so the
UI states that these are unknown. Matching URLs, titles and capture times do
not create a version chain.

Comparison reuses the coalesced research worker and existing action/update
invalidation. Pair, request, admission revision, session and custody checks
protect result installation and exact side opening. Preview extraction scans
each side at most once per page. Comparison bounds persist in the existing
settings file with defaults for older files; malformed/newer files remain
protected. The complete normalized lock graph preserves all upstream package
and dependency selections outside the intended Eidetic revision substitution.

## Check results

Raw commands, output, exit codes, elapsed time and log hashes are in
[model-free-m3](model-free-m3/); `verification-summary.json` separates the source
and gate boundaries. `sources.json` is the initial host checkpoint;
`final-sources.json` hashes the final 24 production/test/Cargo files. Only the
host documentation index changed after the final code checks.

- Provider final review gate: six comparison cases and all 72 no-default-feature
  `eidetic-search` tests passed; three measurement harnesses were ignored.
  `cargo check -p eidetic-search --no-default-features --target
  wasm32-unknown-unknown` passed. The work-bound fix reran both tests and Wasm.
- Initial host final focused gate: `cargo test --locked --lib model_free_ --
  --test-threads=1` passed all 11 cases on `8e8a1b4d`.
- `turnstone-m3-final-01`: existing `cargo test --locked --lib` filters
  `research`, `saved_source`, `saved_search` and `retained_clip` passed 17, 26,
  2 and 18 cases respectively on `8e8a1b4d`. Filters overlap and are not summed.
  One process fixture remained ignored. This run preceded the scoped display
  fixes described below.
- Final review-fix gates on `29fa240e2173cf50eb2f182fb6226071aa9fafc3`: `model_free_comparison_` passed three
  cases, and `model_free_mixed_producers_scope_coverage_and_stale_revision`
  passed one. These cover the amended display, traversal, error and rerun paths;
  the existing full filtered suites were not needlessly rerun.
- `turnstone-m3-inference-final-01`: `cargo check --locked --offline --lib
  --features inference` passed on the corrected final code. This is compile
  evidence, not a live generation run.
- Default and inference dependency boundaries passed for 649 and 797 package
  names. The first offline inspection lacked the newly published source in the
  Cargo cache; fetching it enabled the checks. Every owned Rust/test file is
  within the 600-line rule. Source/document whitespace checks passed; raw compiler
  log whitespace is preserved unchanged with its hash.

The provider warning about an unused `p2panda-auth` patch was already present in
M2. Host compilation also retains existing unused/dead-code warnings. These
checks passed but their output is not warning-free; unrelated pins and modules
were not rewritten to silence it.

## Review corrections and retained failures

The initial provider red run failed on the missing comparison module, followed
by the identity-limit regression. Review exposed repeated long-line comparisons
whose byte cost was absent from the work budget. The failing long-line case is
retained; the fix counts table cells and compared bytes, and pins deletion-first
tie alignment with exact expected spans. Scoped re-review passed.

The host entry-point red run demonstrated that comparison controls were absent.
It ran before receiving completed and lacked `--locked`; subsequent complete
lock equality and locked green gates establish the final graph. The initial
host review found ambiguous labels for the same manifest at distinct nodes and
repeated prefix scans during synchronous preview rendering. One fix wave added
complete identities and a single-pass preview iterator, with a counted-traversal
regression over a long leading line. It also separated withdrawn from unavailable
messages and reset the page on a new run. Both corresponding red failures are
retained. Final scoped re-review found all four findings addressed with no new
issue in the fix.

## Resource and qualification boundaries

All builds used the shared `machine-heavy.lock`, MemoryMax=3G,
MemorySwapMax=0, CPUQuota=150%, one Cargo job and one test thread, disabled
incremental/debug output, a 600-second execution timeout and 600-second lock
queue wait. Limits were unchanged. Local memory was sufficient; no remote
machine was needed for this slice. Final inference compilation took 56.96s and
peaked at 1,388,996 KiB process RSS; the final host focused run peaked at
2,601,132 KiB. Both recorded zero process swaps.

Owner fixtures retain real capture/clip artifacts, reopen exact local bytes,
compare Unicode ranges and reject missing/withdrawn sides. The historical
capture fixture constructs two admissions on one node directly; existing
admission tests separately exercise graph-facet projection. This establishes
owner and reducer behavior, not headed selection or native cold restart.

M3 is complete at its implementation/software-check boundary. M4 structured
queries and M5 headed operation with a fresh, network-disabled, model-free
restart remain open. Existing generated-answer current-input requirements and
all earlier successful/failed qualification packets are preserved.
