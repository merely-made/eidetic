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


### Second host review and storage extension checkpoint

The second backend correction passed 53 focused tests with all 61 frozen source
hashes unchanged (`m4-host-review-round2-freeze-1`, peak 2,753,384 KiB RSS and
zero process swaps). Checkpoint `e3527023` received scoped backend approval;
the UI review was already approved. Saved-history discovery no longer applies
the unrelated query record count, and attachment preflight reserves recovery
scan work before creating new recovery obligations. This was still the
post-fetch storage boundary, before the required engine extension.

Portable error identity was published as Eidetic `a365c1b`. Its first focused
attempt failed to compile because a test payload lacked Debug; the second
passed 147 unit tests, one Fjall integration test and two model-free record
integration tests. Mere `c10356492` preserves Eidetic errors through retained
clip reads; six focused clip tests passed. The Mere lock changed only two
existing dev dependency edges, with no source/version/package selection change.
The Mere gate uses its older Eidetic pin, so it does not alone prove the new
storage-cause accessor reaches Turnstone. Receiving tests remain necessary.

The current Turnstone checkpoints were then rebased onto upstream `7ca35b71`,
producing `f8335284`, `24a0f9a0` and `3113ef7f`. The held storage extension was
preserved in stash `0d48efe0` and restored without dropping any stash. Of its
46 files, 45 retained exact hashes/status/modes; `shell_new.rs` combines the
upstream native-frame event hooks with the storage settings loaded before actor
startup. Both inventories and the reconciliation note are retained here. This
rebase has not yet received a fresh build, and the restored extension remains
uncompiled against incomplete supplier APIs. Earlier receipts qualify their
original frozen sources only.

Official Fjall 3.1.13 fixes journal replay issue #332, where already persisted
entries can be replayed and later flushed into overlapping FIFO runs. A v2
partial-flush/restart test preserves expected values, but that does not test the
reported subsequent FIFO flush failure. Targeted applicability investigation is
part of the storage work. The newer release still lacks the required bounded
normal-tree read contract, and migrating formats would itself read old values
without those bounds. The compatible v2 patch remains unqualified until live
reads and whole-store preparation are integrated, all held tests are enabled,
and independent review and receiving gates pass.


Follow-up source comparison of replay issue #332 confirms that ordinary v2
recovery can replay persisted items. However, v2 FIFO delegates overlapping L0
to maintenance rather than using the v3 assertion, and new Muniment partitions
use Leveled compaction. The precise FIFO poisoning failure is therefore not
established for the pinned engine. No runtime reproduction or unproven backport
was made. The bounded recovery ledger must account for duplicates; preserving
v2 replay semantics does not make its configured limits ineffective.

The upstream write-buffer cap issue #344 and strict-recovery issue #311 remain
open. The bounded journal parser now rejects definitely malformed markers,
value tags, UTF-8 and trailers as corruption. It streams zero padding before
accepting it and keeps only supported incomplete-at-end repairs deferred.
Focused journal tests pass; whole-store commit and live read qualification still
remain. This does not claim to diagnose every possible form of disk corruption.


### Published bounded storage supplier

Eidetic `b39a3dc4d5ef0bd934fc95cb78d5e24e97eff3aa` publishes the compatible
Fjall 2.11.2 / lsm-tree 2.10.4 extension. The original crate archive checksums
and license files were verified; 136 original source, manifest and license
files matched their cached crate archives. Direct path dependencies make Git
consumers receive the derived engines. The workspace lock changes only those
two engine source identities, retaining all other packages, versions and edges.
Standalone engine locks are retained for their focused tests; Fjall's generated
standalone xxhash selection was narrowly aligned to the workspace version.

Both initial task reviews required fixes. Recovery review found temporary
cleanup could unlink a still-held lock, a failed temporary open could delete
an existing store, and each recovered partition's config path clone lacked an
explicit charge. LSM/Muniment review found missing resident decode-work charges,
unadmitted delete keys, incomplete empty-prefix opaque-key coverage, malformed
index extents/pointers admitted too late, Miniz charging maximum rather than
actual output work, and index visits charged after traversal. One consolidated
fix wave per task addressed all nine findings. Scoped re-reviews approved the
fixes and found no new Important or Critical regression. Cross-tree ordering
was inspected: every tree receives admitted replay before any prepared tree
is committed, and the commit performs no later configured-limit checks or
reads of stored contents. Known filesystem path conversions are precharged.

The reviewed 207-file closure is in `m4-storage-review1-sources.json`; its hash
is `ea0a7a2623356c7d042ca1a0db8c83c390a1d1e0f448559e4a18f76f9d87da4b`.
The controller independently rechecked every file and packet log hash before
publication. Locked/offline guarded gates on this closure passed:

- `m4-store-review-wave-lsm-1`: 64 bounded LSM tests, including LZ4 and Miniz.
- `m4-store-review-wave-muniment-1`: 117 native Muniment tests with Fjall.
- `m4-store-review-wave-bytes-1`: two explicit unsupported-bytes cases.
- `m4-recovery-review-fjall-unit-2`: two standalone recovery regressions.
- `m4-store-final-core-search-1`: 150 core and 91 search tests; three search
  tests and two core doctests remain ignored. Peak RSS 881,376 KiB, zero process
  swaps. This checks existing portable callers against the new native supplier.
- `m4-store-final-wasm-1`: no-default core/search Wasm compilation passed.

`storage-gates-index.json` indexes 190 verified raw packets, separating the
reviewed supplier closure from focused/interim evidence. It retains compile
failures, the terminated partial-flush test, subsequent wakeup and write-buffer
fixes, malformed-input failures, and lock setup failures. The unused patch and
unchanged upstream unsafe/deprecation/test warnings remain visible. Git's
import whitespace diagnostics came exclusively from byte-identical upstream
files, retained for patch fidelity; newly authored Rust files stay within the
600-line constraint.

The patch preserves the v2 store format and refuses unsupported blob/bytes
representations. Bounded reads bypass the cache; optional Bloom filters are
omitted during preparation. Existence scans the full matching range, which can
refuse after an earlier match. Temporary cleanup retains the root and stable
lock inode after the last capability drops and is armed only after successful
open. Linux/Rust 1.99.1 has runtime evidence. Relevant installed 1.97.1 standard
library collection and path routines were compared by source, not runtime
qualified. Mac/Windows source accounting and the documented Windows path
refusals do not establish platform qualification. Limits cover named requested
capacities and work, not total RSS, allocator failure, background compaction,
uncooperative external writers, or physical power loss.

Turnstone receiving, final whole-M4 review and M5 product qualification remain
open after this supplier publication.


Turnstone's receiving manifest and lock now select published Eidetic `b39a3dc`
and Mere `c1035649255c934e057fe7eb0e0ebdf8881c1024`. All 1,662 package entries
and dependency edges remain equal after normalizing the 72 intended active
source changes and 21 unused-patch source updates. Fjall and LSM now share the
supplier Git source through their direct path edges. An initial offline check
failed because the new supplier was not cached; `cargo fetch --locked` populated
it. `m4-host-storage-receiving-1` then passed locked/offline metadata and default
and inference dependency boundaries (1,114 resolved packages, two workspace
members, 650/798 package names). The lock hash stayed
`46120659f29809c6fd50e03d55ed2e70e9a41f73fde71a1878421b6e78842ac6`.
Existing unused-patch and cached Servo metadata diagnostics remain recorded.
This is dependency reception, not a host compile or runtime receipt.


### Host bounded-storage receiving checkpoint

The receiving source at Turnstone HEAD `3113ef7f`, plus the 51 changed/new
files in `host-storage-freeze3-sources.json`, passes 65 focused model-free
tests. The controller independently verified the source and receipt log hashes.
`host-storage-gates-index.json` preserves the initial three compiler type
mismatches, the following passing run, and the final frozen run. The first
failure executed no tests. The final run used the existing 3 GiB, zero-process-
swap guard and reported peak RSS 2,748,624 KiB.

Coverage includes the shared operation ledger across a capture, note and sealed
property; exact saved-result worker replay; concrete typed storage failure
through Mere; per-call stored-value and inventory limits; read refusal before
unseal/decode; open refusal with unchanged retained bytes and raised-limit retry;
and editing valid storage settings while an owner error remains visible.
Malformed or newer settings files refuse edits and preserve their bytes;
repairing those files externally remains an explicit limitation. The active
owner limits change only after restart. The mixed result fixture uses a
synthetic assertion receipt, while separate App tests exercise actual graph
attachment. The tight mixed-owner case does not instrument which owner first
encounters exhaustion.

Ordinary saved-source reads now share their operation ledger across legacy and
sealed owners and preserve a sealed-owner startup failure. Research search,
comparison and evidence use bounded adapters. The separate browsing-history
RecallIndex still substitutes an empty stored-body collection after a read
failure; this checkpoint does not establish its completeness. Host task review,
wider compatibility checks, final whole-M4 review and headed M5 cold restart
remain open. These are source-level and library runtime receipts, not product
acceptance or publication of the host draft.


### Host storage review fix wave

The first host task review found three Important issues: `async-trait` was
available only to test builds, storage failures lost their typed category before
UI delivery, and actor-level refusal/retry evidence was incomplete. One fix
wave promotes the existing exact dependency without changing Cargo.lock and
carries typed causes through owner state, actor replies and research jobs. The
separate external inference-service trait still requires formatting at its
existing String API boundary. Known pre-existing compiler and unused-patch
warnings remain; task-introduced unused imports were removed.

The final `host-storage-review-final3-sources.json` freezes 66 changed/new files
on Turnstone HEAD `3113ef7f`. All hashes were independently verified. The
focused run `m4-host-storage-review-final3` passes 69 tests. Both production
library checks (default and inference) and the ordinary route filters passed
on the same production code: research 55, saved-source 28, saved-search 2,
retained-clip 18 with one ignored process fixture. Counts overlap. Subsequent
changes are confined to test modules and their cfg(test) declaration. The
locked/offline receiving verification also passes with the unchanged lock hash.

New regressions assert the exact typed recovery ValueBytes limit through actor
startup/custody adoption and same-directory reopen, unchanged file inventories
after refusal, and success with explicitly raised settings. Mixed query and
saved-result replay refusals assert exact dynamic VisitedItems limits; the
shared snapshot-to-record path demonstrates cross-owner accounting. A sealed
capture refuses before its unseal counter advances. A negative prefix lookup
returns AllocationBytes=0 rather than false on exhausted allocation budget,
then returns valid absence/presence with a sufficient budget. Capture-library
refresh refuses with OutputItems=0 and succeeds with a higher limit; its test
explicitly invokes the same unavailable publication used by the actor and does
not establish automatic actor publication of that particular refusal.

All raw completed packets and source manifests are indexed in
`host-storage-gates-index.json`. The archive preserves intermediate compiler
failures, the comparison-message regression, a zero-match library test run,
and the fixture whose limit also blocked owner startup. Those are not successful
qualification receipts. The final scoped host re-review and whole-M4 review
remain pending at this checkpoint; M5 headed/offline acceptance is still open.


The scoped re-review of this wave accepted the production dependency repair
and the expanded test evidence. It found two capture/ack branches still
stringified retained owner-open failures, and the error-type cut had removed
open/owner/settings/restart guidance and typed comparison-side context.
A second narrow fix wave is in progress. The 69-test receipt above describes
its frozen source, not the subsequent draft. Final M4 approval remains open.


### Broader host gate before final M4 review

The host storage task review approved the third scoped fix at the 68-file
`host-storage-review3-freeze1-sources.json` closure. Its focused run passes
74 tests and both production library checks pass. The controller then ran the
full default library suite on that held source: `m4-host-storage-final-full-lib`
finished with 1,179 passed, three failed and eight ignored, in 302.36 seconds,
peak RSS 337,920 KiB and zero process swaps. This is a failed receipt.

Two failures expose operation labels removed by the typed-error conversion:
source annotation validation and capture recovery receipt write. The existing
recursive context wrapper can restore those labels without losing the cause.
The session-refusal test expected only redraw effects; the additional effect
cancels stale research work, and its shell handler does not grant authority or
write retained state. Its expected effects need to explicitly include that
cancellation while preserving its refusal and byte-preservation assertions.
Read-only diagnosis was supplied to the final cross-layer reviewer; the source
remains held until that review returns a consolidated fix list. These findings
are not silently reclassified as passing tests.


### Published M4 host closure

Turnstone `0089c9d331522343d81a69dcf1f3650ec662f4a4` is published to main,
including the preceding structured-authoring/query checkpoints. Upstream was
refreshed before integration, the exact committed blobs matched all 70 source
hashes in `host-final-fix-freeze3-sources.json`, and the remote main revision
was independently confirmed. Supplier pins remain Eidetic `b39a3dc` and Mere
`c10356492`; the receiving lock hash is unchanged.

`m4-host-storage-final-full-lib-closure` passes **1,183 tests, zero failures,
eight ignored** in 302.01 seconds, peak RSS 339,208 KiB and zero process swaps.
The raw log hash is
`7c96888f7faf38b6b9b10f2cd880d99879f313c23c5af00c3ade7c61df02a9eb`.
Covering model-free, source-capture and session-refusal tests and both production
library checks also pass. The final context-label correction retains exact
native storage causes; tests now inspect those causes through the context
wrapper, preserve the pre-unseal/publication protections, and assert the
acknowledgement label is not attached to legacy reads. The full failed runs
remain in the archive.

The final cross-layer review found no further production defect. Its two
Important findings are closed by the scoped review, the controller's inspection
of the prescribed residual correction and the fresh full gate. The direct
standard-error-source assertion and broader optional serialization parity
matrix remain nonblocking deferred coverage. `host-review-artifacts-index.json`
records exact review packages/reports; `host-storage-gates-index.json` records
raw gate packets and source freezes. Named allocation/work bounds, supported
representations and platform limits remain as documented above. This closes
M4, not the full P4a milestone.

M5 implementation has begun from the published host revision. Preparation
identified the normal `GRAPHSHELL_APP_ENDPOINT` custody route and the marked-root
`custody_payload::retained_custody_receipt_host` fixture, and staged ordinary-control observation and independent copied-
store inspection. No M5 headed seed/restart acceptance result is claimed here.


### M5 supplier integration preparation

Woodshed `b3dfacff93fcf0169097606589a6482b209ed047` adds hosted note action
policy and explicit text-capture availability to the existing Redshank controls.
Its four changed files passed independent source review and the nested surface
library gate: **111 passed, zero failed or ignored**. The private-only host can
omit unsupported Edit/Delete controls while retaining the owner editor and
privacy action. Absence of an explicit capture capability preserves the existing
transport-derived behavior; every policy still requires a current item.

Mere `a4cbd4baf3411b597bdcfc13653de0444b5d480d` completes three Graphshell
adapters for the typed clip-read error. Independent source review and the locked
offline Graphshell library check pass. The marked-root custody fixture also
builds. The earlier missing Git checkout failure and original adapter compile
failure remain failed preparation receipts; a compiled fixture is not a running
service.

Both suppliers were refreshed from upstream before publication and their remote
main revisions independently confirmed. The host receives six Woodshed, 62 Mere
and 33 Genet lock source references, preserving all other lock bytes. Genet
`c0144fe59dea449519cede190bed8662f661fc22` is the single geometry/accessibility
commit required by the new Mere geometry API. Eidetic remains `b39a3dc`.
Locked offline metadata resolves 1,114 packages; default/inference link-boundary
checks pass at 650/798 package names. Host compilation after this reception,
the note adapter and the native acceptance sequence remain pending.

The startup preflight uses the custody-payload fixture, not the C1 receipt host:
the latter lacks the required payload-custody configuration. The isolated
Wayland/render-device descriptor probe succeeded, but launched no application.
Review found scope-cleanup, writable mount-input validation and evidence-label
issues in the staged startup harness; they must be corrected before its run.
No headed capture, real-note retention or fresh-process M5 acceptance is claimed.

The initial 19 completed preparation packet/log pairs, supplier reviews and
receiving records are preserved under `model-free-m5/`;
`preparation-gates-index.json` records their verified checksums and exit codes.


### M5 evidence archive update

The completed M5 packet/log pairs added since the initial 19, the scoped note and
custody review materials, and the five completed native-preflight attempts are
indexed in `model-free-m5/additional-evidence-index.json`. It records source and
archive SHA-256 values; failed receipts remain included. Native manifests,
cleanup records and synthetic observations are preserved without binaries,
profile stores, graphics caches, wallets or custody seeds. The D frame remains
at its external testing path; its path and SHA-256 are recorded in the index.

On the local Turnstone working tree based on upstream `1f371cbd`, the full
`authoring,inference` library gate passed **1,262 tests, zero failures, 17
ignored**. The archived packet is `m5-host-full-after-upstream-1.json`; its raw
log SHA-256 is `62dd8866467f1be84f13cb37683d8a1fb39bb2d79f6bb080e0fafb19f145d9c9`.
This is local WIP evidence. Turnstone commit `293eb103` and Mere commit
`8fec5603` remain local because their pushes failed; remote main remains
Turnstone `1f371cbd` and Mere `a4cbd4b`. Neither commit is described here as
published.

The scoped note-host fix-2 review is approved; its focused gate reports **34
passed, zero failed, one ignored**. The real-custody service-lease fix review is
approved; the corrected lease regression passed **1/0**, and the post-lease
custody fixture passed **3/0**. The earlier failed attempt and review materials
are retained alongside them.

Native preflight attempts A through D failed at different stages: A could not
enable loopback; B reached custody but expected the wrong route-refusal variant;
C started the app but produced no frame before its rendering deadline; D rendered
but failed the custody-principal check. None qualifies M5. Attempt D selected
AMD Radeon Graphics (RADV Renoir) on Vulkan; the scenario recorded 66 frames,
one nonblank capture and zero blank captures. The runtime receipt still reports
`custody_principal: null`, and the launcher failed because the GUI and probe
principals differed. Its screenshot is preserved by external path and hash only.
Attempt E later passed the empty-profile startup/preflight gate. The app and
probe reported the same family custody principal, the probe observed
`closed-during-open`, offline acquisition, corpus publication and generation
counts were all zero, and cleanup confirmed the scope cgroup empty. Its safe
manifests and logs are indexed in `model-free-m5/additional-evidence-index.json`;
the frame remains external with its path and SHA-256 recorded there. This
qualifies startup/preflight only. No retained note, fresh-process restart or
ordinary-use M5 acceptance is claimed, and the full M5 gate was not run.


### M5 observer, readiness, fetch-wait and failed collection evidence

The observer review chain, readiness/fault integration, fetch-wait review, and
associated focused receipts are indexed in
`model-free-m5/additional-evidence-index.json`. The initial observer review and
fix1 review requested changes; fix2 was approved for scoped source review. Its
three cleaned privacy/projection tests each passed 1/0. Readiness/fault plumbing
and fetch-wait integration were also approved for scoped source review only.
Focused address-history tests passed 2/0, shared-receipt refusal passed 1/0,
source-target tests passed 2/0, and the correlated fetch-to-capture-action test
passed 1/0. Earlier red and integrity diagnostics remain preserved; the
read-only inspector's real-store checks and fault injection were not run, and
`READY_FOR_FAULT_COLLECTION` remains false.

Failed intact collections A, B and C are archived as safe records in the same
index. A failed at note-root setup: the research-note editor was not live, the
attempted note insert found no editor, and no reopen ran. B created the ordinary
Knot note but timed out waiting for the admitted A capture action because the
scenario wait did not drain fetch updates. C timed out because its browser
command target differed from the requested source target. Each failed scope
verified its cgroup empty. Frame images and synthetic fixture media are not
copied; their external paths and hashes are recorded. Binaries, stores, caches
and custody identity/vault data are excluded.

These receipts do not change the M5 boundary: observer/source checks and startup
E are bounded evidence; no intact collection reached successful save and fresh
reopen, no native fault injection ran, and full offline M5 acceptance remains
incomplete.


### M5 source-save interaction diagnostics

The additional safe manifests and logs for failed intact collections D and E,
plus source-click diagnostics A and B, are indexed in
`model-free-m5/additional-evidence-index.json`. D failed when the row-targeted
source-save click missed. E failed because the offered button was below the
viewport. Neither saved the A source, and neither is an acceptance run.

The reviewed scenario-only map-focus, explicit-button and scroll diffs, reviewed
scenario, and current diagnostic-only registration lock are archived with those
records. Diagnostic A observed `interaction-missed` and an empty capture-attempt
list. Diagnostic B used ordinary scroll input to reveal the button, then the
ordinary click saved and kept the A capture; its capture JSON is preserved. Both
diagnostics intentionally stopped at their diagnostic assertion. The before-
click and other frames remain external with paths and hashes in the index.

These results narrow the next full-collection work to the ordinary source-save
interaction. They do not qualify the intact sequence, fresh-process reopen,
fault injection or full M5 acceptance; the next full collection remains
pending.


### M5 research-search and passage-clip diagnostics

Safe run records for intact collections F, G and H, the research diagnostic, and
passage-clip diagnostics A and B are indexed in
`model-free-m5/additional-evidence-index.json`. The shared research-response
drain packet and root review, focused green-2 receipt, earlier failure receipts,
and the reviewed fault-ledger-fix1 packet/tests/reviews are archived there too.
The shared drain passed its focused real-worker test and root approved the
scoped integration; it does not establish headed Shell behavior. Fault-ledger
fix1 has no remaining scoped review findings, but its guard remains false and
no native fault injection ran. The changing integration verifier is excluded.

F and G each recorded two source captures, then failed their current-result
search waits. H displayed the actual retained A1 reader, but the scenario used
an incorrect expected passage substring and then missed the citation/readback
step. Research diagnostic A observed a current Complete search with one Capture
hit. Passage diagnostic A showed the correct full quote but missed the clip
action. Diagnostic B used ordinary expand/scroll input, after which the clip
was reported saved. The note readback is still unqualified. All diagnostics
stopped intentionally; screenshots and synthetic fixture media remain external
with hashes in the index.

These runs do not qualify a full collection, note citation/readback, fresh
reopen, fault injection or M5 acceptance. The next full collection remains
pending.


### M5 limit observer, cold reader and receipt integration source reviews

The limit-refusal observer and cold-reader source review packets, focused receipts,
reader-hoist diff/root review, and the final receipt-integration fix2 package are
indexed in `model-free-m5/additional-evidence-index.json`. The limit observer
review first found a refusal invalidation gap; the red roundtrip reproduced it,
and the green roundtrip cleared it. A prior pre-dispatch explanation of the
record limit is disproved: enumeration dispatches to the worker, which enforces
`max_records` on admitted records and returns the exact refusal. Focused tests
covering refusal binding, roundtrip invalidation, retained-reader scope,
source-target wait and hit selector passed as recorded. These are focused tests
and source review, not evidence of a native worker limit hit or headed cold
reopen. The cold-reader scoped review was approved; the later hoist review
approved moving unchanged admission enumeration out of the live-reader loop.
Its 39-file source manifest matched, while the full host gate was still running
and cold reopen remained pending.

All four receipt-integration files and all three successive review reports,
including the final root 42-test log, are preserved. Fix2 was approved for its
scoped citation-passage and retained-reader checks; the verifier remains guarded
as draft. The fault-ledger fix1 packet records 17 passing focused tests and an
approved scoped review, while the fault guard remains false. The current
registration lock is preserved separately from the earlier archived version.
There is still no successful native intact acceptance or native fault injection.

The archive contains some runnable components, including the receipt assembler
and verifier, the fault runner and scenario, tests, and selected native
preflight sources. It does not yet contain the complete matching main M5
launcher, verifier, phase helpers, seed/reopen scenarios and supporting fixture
source bundle outside ignored SDD. The final qualified harness must be archived
as an exact source bundle with its receipt before it can be called
reconstructible. Full M5 acceptance remains incomplete.


### M5 reader-open event evidence and focused host gate

The full authoring/inference library gate receipt and the local reader-open
focused receipts are indexed in `model-free-m5/additional-evidence-index.json`.
The full gate passed 1278/0/20 at Turnstone source
`fe015c5df394f7be514609ebe2e5ecb25528f618`. The subsequent local
`cf459cd832abc877c79ae3c68e61eb7014ccf533` event/Peek wording delta has one new
focused test passing 1/0 and two existing address-history tests passing 2/0.
These focused checks do not replace a full gate for the later source.

Failed intact I and citation-click diagnostics A and B are archived as safe
records. Intact I clipped exact A1 into the real Knot note but stopped after
the citation opened Peek instead of immediately producing a reader. Diagnostic
B followed the ordinary Read control: its `WorkbenchTileOpened` event named
the exact A1 address, and a separate reader observation was Ready for A1 with
the exact manifest, text hash, selector 0..57 and full passage. Both diagnostics
are intentional stops, not acceptance runs. Failure records and cleanup are
retained; screenshots and synthetic HTML fixtures remain external with paths,
hashes and sizes in the index.

The reader-open source-review packet, scenario wording/seed delta, updated
reader-event fault scripts and current registration lock, failed registration
fixture log, 17-test log/review, and final four-file reader-event receipt
adapter with root review are preserved at distinct paths. The reader-event
source and receipt adapter reviews are scoped approvals. The fault guard remains
false and the receipt remains DRAFT. No intact run completed the full
seed-to-cold-reopen acceptance sequence, and no native fault run completed.
The main fresh build was still running, so its incomplete outputs were excluded. Full M5 acceptance remains incomplete.


### M5 failed intact J/K and note-control diagnostics

Safe records for failed intact J and intentionally aborted K, Knot editor
diagnostics A-C, Redshank note-control diagnostics A-E, and the reviewed
scenario deltas are indexed in `model-free-m5/additional-evidence-index.json`.
J failed at note insertion because no focused text editor was active. K was
intentionally aborted after the complete note-search observation lacked the
required Note producer; the saved abort reason explains why following actions
would only cascade. Its failure and cleanup receipts are retained.

The Knot editor diagnostics failed at their successive editor/text or stop
assertions; their scenario outputs and cleanup records preserve the individual
boundaries. Redshank controls A-D also stop at their recorded control mismatches.
Diagnostic E showed the selected Note action absent while private, then reported
shareable retention Complete with count 1. Playback is still unqualified, and E
stopped intentionally. No diagnostic here is a complete intact or reopen
acceptance. Screenshots and synthetic HTML fixture files remain external with
path, hash and size in the index. Binaries, media, stores and custody identity
data are excluded. Intact L was still running and was not archived. Full M5
acceptance remains incomplete.


### M5 intact L timeout, Read document review and publication update

Safe manifests, logs, configuration, scenarios and selected capture/readback
JSON from intact collection L are indexed in
`model-free-m5/additional-evidence-index.json`. Its configured 450-second app
deadline expired after authored step 4. The seed phase did not complete, so no
reopen ran. Cleanup terminated the app, custodian and fixture server and later
verified the scope cgroup empty. This is a timed-out collection, not a semantic
product failure. Frames and synthetic HTML fixtures are indexed by external
path, SHA-256 and size; binaries, media, stores, caches and custody identity
data are excluded. L does not qualify save/reopen or M5 acceptance.

The current `missing.scn`, its hash manifest and the scoped Read document review
are preserved at `model-free-m5/review-packets/missing-document/`. Review found
no issue with adding the ordinary Read document action after opening the
research note. The existing 17 Python fault and ledger tests also passed. No
native fault scenario ran; headed citation visibility remains unverified, and
`READY_FOR_FAULT_COLLECTION` remains false.

Publication status is now resolved. The earlier failed push attempts recorded
above remain accurate for those attempts; the exact Mere `8fec5603`, Turnstone
`cf459cd832abc877c79ae3c68e61eb7014ccf533` and Eidetic `0919a72` commits were
subsequently pushed via idle s-pc and independently fetched here. Archived
publication metadata records the exact heads, bundle hashes and unchanged
checkout states. The publishing script is retained for provenance; bundle
archives and credentials are excluded. Publication is source state only and
does not qualify M5.


### M5 intact M, empty-scope diagnostic and readiness evidence

Safe records for failed intact M and empty-scope diagnostic A are indexed in
`model-free-m5/additional-evidence-index.json`. M emitted 14 authored frames and
all 12 structured-operation observations. It then failed at the first Withdrawn
record-history row, which was absent while the interface remained in Inspect view;
subsequent assertions cascaded. The structured snapshot records 14 input records before
withdrawal and 13 after. Independent copy inspection and reopen were not run, and
the scope cleanup receipt verifies the cgroup empty. This is a failed collection,
not M5 acceptance.

Empty-scope diagnostic A reproduced a new search being cancelled by stale
structured-result invalidation. After a saved structured result, the scope result
reported `search: null` and an empty chosen scope, then timed out waiting for a
current result. Cleanup verified the cgroup empty. This is a diagnostic result,
not acceptance.

The corrected visual review, stable readiness source/review packet and source
helper, focused readiness gates, and current registration-lock-waits diff are
indexed. The visual review marks the diff result visible but clipped; Knot
metadata overflow and the selection not being visible remain blocked. Review and
focused gates are scoped source/software evidence only. The changing scope-order
and Knot-fix packet and running gate outputs were excluded. A complete seed-to-reopen run
and final acceptance remain open.

The completed M provenance receipts and logs are retained without build binaries.
Readiness publication metadata and its script record Turnstone
`c2f69b743a0cb3d637b32fb2ef7f4c164fb60d8a` and Eidetic
`a251467aa4044ff625f38264cfff5f0bcd37c0f9` as published and independently
fetched. Bundle files and credentials are excluded. Earlier push failures remain
part of the historical record; this publication update does not qualify M5.


### C2 author-save and remaining-waits review packets

The stable C2 author-save and remaining-waits source/review packets are indexed
in `model-free-m5/additional-evidence-index.json`, with their focused red, green,
parser and refusal receipts. Author-save red-1 selected zero tests; red-2 failed
on the missing status field; green-1 failed to compile; green-2 passed the
owner-backed test 1/0, and the parser gate passed 1/0. The remaining-waits red
case failed on the absent idle field; the owner-backed green, parser and bound
refusal gates each passed 1/0. These are focused software checks. The native
seed, reopen, inventory and full M5 acceptance remain unproven.

The combined source diff and nine-file manifest are retained with the exact
`src/shell/drive/shell_research_waits.rs` helper. The helper hash matches the
manifest at Turnstone HEAD `180df8758ba7c3be3d796287e1fcc0464a59dc9d`. The
other eight tracked files are represented by the archived combined diff and
manifest. The changing scope-order and Knot-fix packets remain excluded.

### Scope, note display and scenario-filter follow-up

Turnstone `180df875` fixes the order of stale structured invalidation and the
replacement search, and uses the existing pane scroll machinery for the Knot
editor. Scope-order and geometry checks each pass; existing Knot tests pass
24 with 4 ignored, and research search tests pass 7. The first red scope log
crossed concurrent Knot edits and is not a frozen-source receipt. Commit
`d0d93da` changes only selection background transparency. The real native frame
shows readable selected words, and the exported payload is exactly 24 bytes,
with private surrounding canaries absent. This establishes host readability,
not the suspected underlying Genet paint-order cause. Fresh Genet upstream
still equals the pinned revision. Empty-scope diagnostic B separately proves
that the replacement search completes with no sources or hits.

Collection N was interrupted without a normal collector cleanup receipt. Its
exact native scope was subsequently observed absent; the interruption cause is
unknown. Collection O failed normally after all 14 authored records and 12
operation observations. The scenario attempted SourceLookup while retaining
`m5:withdraw`, a record-only filter. A short native diagnostic captured the
exact Unsupported refusal. Clearing the filter before the query produces a
current Complete saved SourceLookup with all filters null, chosen-empty scope,
zero sources/records/rows and matching saved/content manifests. These diagnostic
runs deliberately stop and have verified empty native cgroups. Full collection
P uses the corrected scenario with the same `d0d93da` build cohort.

The additional-evidence index now retains 1,643 copied-file references, all
independently checked against their byte counts and SHA-256. It includes the
new source reviews, focused gates, diagnostics, interrupted/failed collections
and publication metadata. It excludes executables, stores, custody keys and
Git bundles. UI publication records confirm Turnstone `d0d93da` and Eidetic
`450ceea`, with unchanged remote checkouts. Full native M5 remains incomplete.

The provisional `model-free-m5/harness/` bundle preserves 40 exact source and
fixture files with a manifest, a documented build guard and detached controller
recipe. A separate review checked sibling imports, pinned validator hashes and
exact synthetic audio reconstruction. Copied harness unit suites pass 4 + 3 +
17 + 42 tests. The fault driver remains disabled and final verification remains
DRAFT. This is reproducibility and helper-test evidence, not native acceptance.
