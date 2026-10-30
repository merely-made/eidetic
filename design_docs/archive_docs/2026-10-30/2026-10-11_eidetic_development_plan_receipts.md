# Model-free retained research receipts, 2026-10-30

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


## M4 provider checkpoint

Canonical continuation: [P4a M4](../../eidetic_docs/implementation_strategy/2026-10-11_eidetic_development_plan.md#m4-answer-the-supported-structured-questions).
This checkpoint establishes the portable provider. Turnstone host authoring and
query integration remain in progress; it does not close M4 or M5.

Upstream was fetched and fast-forward eligibility checked before work. The
provider started at `848a4bdbfbce62d952cf46a9ae87d785fb949405`; implementation
`b24c580` and review correction
`2f55c137f25e619e8f7be2b7dbb76d15e081bc32` are published on Eidetic main.
The Turnstone receiving base is `cce607d38f5084a8b1ddec3361f7fd04e301b000`.
All Eidetic pins were advanced together. Parsed Cargo.lock package and dependency
selections exactly match the host base after substituting the intended Eidetic
revision. Default and inference dependency-boundary checks passed for 649 and
797 package names.

The portable provider defines typed properties, exact definition-manifest
references, sourced assertions, lifecycle observations, complete snapshots,
explicit queries/settings and frozen results. Its evaluator supports source
metadata, assertion counts, lookups, equality groups, explicit-ID joins,
comparison and checked sums. Incomplete enumeration refuses totals and absence;
counts never consume a lexical shortlist. Comparison preserves entity, context,
status, cardinality and valid-time distinctions. It does not interpret arbitrary
prose. The operation version is `eidetic.structured-records/v2`.

All 30 provider gate packets and their original logs are retained in
[model-free-m4](model-free-m4/). The packet log hashes were verified before
copying. `provider-manifest.json` records the final 11 source-file hashes and
all packet statuses. The final reviewed-source gates are:

| Packet | Command after guarded wrapper | Result |
| --- | --- | --- |
| `eidetic-m4-review1-green` | `cargo test --locked -p eidetic-search --no-default-features --test model_free_records` | 19 passed |
| `eidetic-m4-review1-core` | `cargo test --locked -p eidetic --no-default-features --test model_free_records` | 2 passed |
| `eidetic-m4-review1-full` | `cargo test --locked -p eidetic-search --no-default-features` | 91 passed, 3 ignored |
| `eidetic-m4-review1-wasm` | `cargo check --locked -p eidetic-search --no-default-features --target wasm32-unknown-unknown` | passed |

The first red tests failed on missing interfaces. Subsequent red cases exposed
ignored filters, collapsed grouping distinctions, missing strict schema helpers
and insufficient repeated-validation accounting. Initial review found pairwise
value work omitted from the bound and aggregate rows cloned before their output
budget check. The final fix charges identity/value bytes before comparisons,
compares typed values directly, and measures borrowed row representations before
materializing output. Scoped re-review approved both corrections. Exact and
undersized output-budget regressions cover counts, groups, long identities and
numeric sums; source review establishes the allocation ordering.

These gates used the shared machine-heavy lock and unchanged limits:
MemoryMax=3G, MemorySwapMax=0, CPUQuota=150%, one Cargo job and test thread,
disabled incremental/debug output, and 600-second execution/queue bounds. The
existing unused p2panda-auth patch warning remains. A read-only SSH resource
check found s-pc idle; no remote M4 build or transfer was needed.

The core save/load fixture uses an XOR test sealer. It verifies schema and
sealed-storage plumbing, not cryptographic confidentiality. Host custody,
durable journal integration, headed product use and network-disabled fresh
restart require their own evidence.


## M4 host integration and supplier follow-through

The first host checkpoint was `563fe91bbb8c3273259dee82f22a151bbf9ecd5f`.
It passed 44 `model_free_` tests on a frozen 57-file source set before task
review. Turnstone upstream then published `720f59f8`, the Djinn application
client ownership migration. The checkpoint was rebased to `0d71a2f1` and all
25 uncommitted review-fix files were restored byte-for-byte from retained stash
`614a8c0`; the earlier M2 stash was preserved. The original hashes and restore
proof are in `model-free-m4/upstream-720-reconciliation.json`.

The bounded live resource-pair supplier is Mere
`229f277a252440a8e47aefe367febca3cf502bbb`. Its first replacement regression
exposed that durable snapshots omit session-only arrangements; live recovery
must preserve them. The corrected API serializes borrowed selected assertions
under byte and edge bounds before cloning, retaining all live assertion kinds.
The rebased supplier passed 528 kernel tests; independent review approved it,
with a deferred minor request for broader optional-field serialization parity
fixtures. `mere-bounds-sources.json` records source hashes and all seven
successful/failed gate packets. Later Mere changes through `4508898eb` are
documentation only; Turnstone retains the tested code revision.

The Turnstone receiving lock replaced 61 Mere source hashes without adding,
removing or changing dependency edges. Eidetic's subsequent explicit-sync API,
`a025008c36cc21c0afa63682e044cf1ca99403b4`, was received coherently in every
manifest and lock source occurrence. That second comparison changed eight
Eidetic package sources, with no package or edge changes, including normalized
unused-patch records. The graph contains 1,662 lock package entries. Locked
metadata resolves 1,114 packages and two workspace members. Default/inference
boundary checks pass for 650/798 package names. An initial offline metadata
attempt lacked the exact `www.github.com` supplier cache; fetching locked
sources populated it without changing selections. Existing unused-patch
warnings and the cached Servo malformed tidy-fixture diagnostic remain; the
final metadata commands return valid JSON and exit zero.

### Review corrections and authority

Initial UI review identified omitted source contributors, inaccessible frozen
inputs and full values, and repeated full-result traversal. The first fix made
source, definition, record, lifecycle and settings inputs inspectable with
indexed pages and Unicode-safe field chunks. Its scoped review accepted those
fixes but found that active assertions had lost their live withdrawal route.
The second fix adds a generation-checked current-assertion inspection route,
keeping the saved record and current lifecycle distinct.

Backend review identified completed receipt history exhausting active budgets,
unbounded decode paths, and recovery copying unrelated graph payloads. The fix
separates pending receipt keys from completion/idempotency keys, applies
post-fetch envelope and manifest checks before unseal/decode, accounts for the
complete selected input cumulatively, and uses Mere's bounded live-pair API.
Saved results replay their frozen historical inputs; a new query excludes
withdrawn assertions, and current inspection exposes the current state.
Unpublished v1 receipt keys refuse with their bytes preserved rather than being
silently interpreted as the new protocol.

Turnstone's current graph owner is Mere's semantic-statement kernel, with
process-local HostJournal capture. The canonical graph snapshot persists
attribution, basis and tombstones. The existing session writer holds an OS
file lock, checks directory/lease ownership, rejects loss of recorded operations,
and propagates facet and graph errors. Its graph writer syncs a unique temporary,
renames it atomically and syncs the parent directory. This is distinct from a
durable event journal; intermediate cross-restart event replay remains open.

The unchanged Muniment Fjall adapter initially exposed only buffered writes.
Explicit `FjallStore::sync_all` now delegates to `PersistMode::SyncAll` and
propagates failure. Its focused red failed on the missing API; all eight Fjall
backend tests then passed. Independent source review confirmed the selected
mode reaches `File::sync_all`. The subprocess fixture exits without dropping
the store and reopens its synced receipt. It does not simulate power loss or
inject a physical storage failure. Host mutation replies must follow the sync;
completion must be synced while pending recovery evidence still exists, before
that pending key can be removed.

### Interim evidence retained

The raw host attempts and source checkpoints are retained in `model-free-m4`.
`host-interim-packets.json` distinguishes compilation failures, interim passes
and frozen qualification. The first integration attempts failed with eleven,
then six compiler errors. A later pass ran 29 tests, then another ran 30 while
source edits raced its launch; that latter result is explicitly interim. The
first combined freeze failed on an Inventory call arity; the second passed 44.
The first review-fix freeze failed at compilation because replay-budget lines
were placed in the evidence validator; no tests ran in that attempt.

Correction to an earlier narrative: the `m4-host-definition-green-1` wrapper
was cancelled, but its child later completed one passing test and exit zero.
Its raw log is retained (65.05 seconds, 2,622,248 KiB peak RSS, zero swaps).
There is no JSON completion packet or source-hash checkpoint for that orphaned
wrapper, so it is not final-source qualification.

### Remaining allocation and product boundaries

Fjall's regular-tree `size_of` calls `get`; key iteration can materialize values,
and value-block decompression precedes a public size result. A wrapper cannot
honestly guarantee a bound before that initial allocation. Host limits now
protect subsequent unseal/decode and cumulative retained inputs, but initial
Fjall reads/listing and earlier source-loader temporary allocations remain open
storage-owner work. No alternate store or migration was introduced. The user subsequently chose to extend M4 to close this gap and requested
checking new releases first; design work for that extension is in progress.

Manual authored records use the active Personae identity and distinct authored
nonces. Repeated projection of one exact binding collapses; different bindings
sharing a nonce refuse ambiguity. Copying source text does not establish
independent corroboration. Clip/note capture time stays unknown because mutable
manifest retention time is not an immutable capture event. Existing explicit
All scope remains the default; an explicitly empty chosen scope stays empty.

M5 headed ordinary use, absence of model files, disabled network acquisition,
and independent-process cold restart are not established by these software
fixtures. No physical power-loss qualification is claimed. The source changes
use the same shared heavy-work lock and existing 3 GiB/zero-swap/150% CPU limits.
Read-only SSH resource checks found S-PC idle, but no M4 source transfer or
remote build was performed; its existing dirty worktree remains untouched.

The reviewed-fix checkpoint `909715c9` passed 51 focused `model_free_` tests
with all 60 recorded source hashes unchanged (packet
`m4-host-review-fixes-freeze-2`, peak 2,767,988 KiB RSS and zero process swaps).
UI round-two source review approved the live inspection/withdrawal fix. Backend
round-one review retained two related history/recovery findings: saved discovery
refuses all entries above the query record count, and a new attachment can exceed
the graph scan work/byte bound before recovery captures its scope. These are
being corrected; this checkpoint is not M4 completion or publication evidence.
