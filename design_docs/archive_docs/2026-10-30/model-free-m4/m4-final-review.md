# M4 final cross-layer review

**Verdict: Needs fixes.** No Critical finding. One Important production diagnostic regression and one Important validation failure require a consolidated host fix wave. The two previously deferred test gaps remain Minor and nonblocking. I found no additional correctness defect in the reviewed portable evaluator, graph/sealed persistence protocol, source-authority checks, or bounded storage implementation.

Reviewed 2026-10-30, read-only. No builds, tests, code changes, or subagents were used for this review. This report is the only write.

## Frozen scope and evidence

The scope is the five `m4-final-{eidetic,fjall,lsm-tree,mere,turnstone}.diff` packages and the final dispatch section of `m4-final-review-brief.md`, with the rulings and task-review history in `progress.md`. Supplier source is published Eidetic `b39a3dc4d5ef0bd934fc95cb78d5e24e97eff3aa`; Mere is published `c10356492`; Turnstone is `3113ef7f` plus the 68-file `testing/model-free-m4-20261030/host-storage-review3-freeze1-sources.json` freeze. The controller independently verified hashes. Unchanged vendored engine source is excluded by the package's exact upstream-archive comparison.

The controller supplied passing focused 74-test and default/inference production-library receipts. The subsequent full default library receipt is **failed: 1,179 passed, 3 failed, 8 ignored**, exit 101, with zero swap. I inspected its failures and corresponding current source: `/home/markik/Code/testing/custody-contract-20261029/m4-host-storage-final-full-lib.log:2697` through the result at line 2722. These results supersede the brief's statement that the full gate was still running. M4 is not approved on the frozen source; M5 headed/offline acceptance remains separate and unclaimed.

Path prefixes below are:

- **Turnstone:** `/home/markik/Code/worktrees/turnstone-model-free-m1-20261029/`
- **Eidetic:** `/home/markik/Code/worktrees/eidetic-model-free-m1-20261029/`
- **Mere:** `/home/markik/Code/worktrees/mere-model-free-m1-20261029/`

## Important findings

### I1. Preserve the failed operation when converting source/capture errors to typed errors

**Primary locations:** Turnstone `src/trail_memory/verify_source_document.rs:59` and `src/trail_memory/source_capture_receipts.rs:33`.

The conversion replaces contextual error mapping with bare `?` or `HostError::from`. A rejected Fleece annotation now loses the `source annotation validation` context; a failed final recovery-receipt write loses `capture recovery receipt write`. The full gate reaches these errors but fails the existing diagnostic assertions at `src/trail_memory/source_capture_tests/deposit_failure_at_each_real_write_reopens_preserves_cases.rs:217` and `src/trail_memory/source_capture_tests/partial_receipt_recovers_original_non_utf8_html_decoding_cases.rs:30`. These are real lost diagnostics, not evidence that invalid annotations are accepted or that a failed deposit is reported successful.

This matters at the recovery boundary: a lower-level backend or validation message alone does not identify whether verification, payload deposit, association, or durable receipt handling failed. The same mechanical removal occurs at:

- `src/trail_memory/verify_source_document.rs:33`, `:47`, `:92`: manifest, payload, and raw validation.
- `src/trail_memory/preflight_source_artifact.rs:13`, `:27`: preflight manifest and payload reads.
- `src/trail_memory/capture_source_document_in.rs:104`, `:144`, `:147`, `:154`, `:165`: raw preflight/deposit, schema bootstrap, annotation deposit, association.
- `src/trail_memory/source_capture_receipts.rs:59`, `:86`, `:94`, `:128`, `:173`, `:183`, `:191`, `:216`, `:260`, `:288`, `:292`: annotation, retry raw/decoded body, receipt/deposit/acknowledgement reads and writes, recovery inventory and per-receipt reads.
- `src/trail_memory/sealed_capture.rs:53`, `:60`, `:95`, `:97`, `:119`, `:152`, `:160`, `:175`, `:492`: sealed receipt and artifact read/write/validation/inventory operations.
- `src/trail_memory/emit_capture_recovery.rs:49`, `:60` and `src/trail_memory/emit_sealed_capture_recovery.rs:48`, `:54`: recovery failures no longer carry their request/verification context.

**Fix:** restore the meaningful operation labels through `SourceReadError::context`, including the analogous converted paths in this list. Keep the original cause intact. `src/research_storage.rs:384` already wraps context recursively and `:396` retrieves the underlying storage error through that wrapper; comparison-side errors demonstrate this usage. Do not repair these tests by dropping their diagnostic assertions, and do not revert to stringifying the cause before actor/job boundaries.

**Done condition:** the two existing failure/reopen tests pass; focused checks show both the displayed operation label and the exact `StoreError` kind/limit survive representative context wrappers. Preserve the existing failure-closed and receipt/reopen assertions.

### I2. Correct the refusal test's effect contract while preserving cancellation

**Location:** Turnstone `src/app/session_persistence_tests/host_session_refusal_blocks_save_fork_background_and_cases.rs:19`.

The test allows only `Redraw`, but both refused-adoption exits return `CancelResearchSearch` plus `Redraw` at `src/app/session_lifecycle/app_adopt_session.rs:50` and `:74`. I inspected the comparison baseline `7ca35b71` as well: it already has these cancellation effects. This is an inherited test expectation mismatch exposed by the full gate, not a newly introduced graph-write or authority regression.

The named risk was whether cancellation performs a forbidden write after refusal. Current shell dispatch at `src/shell/effects/shell_effect_control_content.rs:12` calls `Worker::cancel`; `src/research_search.rs:527` only advances the worker serial and clears pending requests/results. Both refusal exits precede the session persistence work. Cancellation is appropriate for preventing stale results from the previous context.

**Fix:** explicitly require cancellation and redraw and reject every other effect in this assertion. Retain the remainder of the fixture, including refusal state, blocked save/fork/background behavior, graph replacement, and exact preservation of the invalid session bytes. Do not remove cancellation to satisfy the stale assertion.

**Done condition:** this fixture and the full default library gate pass after the consolidated host fix. The full gate must be rerun by the controller on the new freeze; this review does not substitute for it.

## Deferred Minor findings, triaged

1. **Direct standard-error source test.** Eidetic `crates/eidetic-core/src/lib.rs:189` correctly returns the stored `StoreError` as the standard error source. The getter/conversion test at `:361` covers the exact typed cause and unchanged display, but not `std::error::Error::source(...).downcast_ref::<StoreError>()`. Add that assertion, plus the semantic-error `None` case, when next extending this coverage. The implementation is direct and no failure is observed; this does not require a supplier republish for M4 approval.

2. **Borrowed serialization optional combinations.** Mere `crates/graph/graph-kernel/src/graph/snapshot/bounded_resource_tests.rs:123` checks exact borrowed/live byte parity for its current fixture. It does not cover all optional combinations. I compared `graph/resource_pair_bounds.rs:198` with the current canonical `persistence_edge.rs:277` and `graph/snapshot/to.rs:278`, particularly explicit-null first-asserted time and empty basis with a label. The conditions agree. A small parity matrix for optional scope/provenance/times/withdrawal/basis fields would guard future drift; no current undercount or serialization mismatch is demonstrated.

## Cross-layer assessment

| Contract | Review conclusion |
| --- | --- |
| Portable typed records and evaluation | Definitions, revisions, typed values, explicit date/offset semantics, lifecycle and exact evidence identities are validated. Evaluation uses the complete selected snapshot, preserves unknown/conflicting values, and refuses incomplete or over-budget inputs/outputs. Source operations are not lexical top-k substitutes. |
| Graph and sealed persistence | Durable graph assertions/tombstones remain authority; the host journal is process-local. Nonces, exact graph bindings, pending receipt inventories, sync fences and post-save acknowledgement prevent partial work from silently becoming a complete result. Recovery restores an exact assertion additively against the live bounded pair, preserving neighboring assertions. |
| Source authority and UI | Current generation, graph owner, custody and admission checks gate worker completion and source opening. Frozen historical input inspection is distinct from current assertion/evidence access. Complete contributors and full field chunks are reachable; result indexing and borrowed previews avoid the previously reported whole-result navigation scans. |
| Storage recovery | Read-only whole-open preparation admits journals, partition metadata, replay objects and required indexes before cleanup/truncation/background work. Definite malformed journal bytes are corruption; supported incomplete EOF tails are only committed after admission. The common owner lease follows snapshots/iterators; successful temporary cleanup preserves the coordination inode. |
| Storage reads and errors | The owner installs ordinary-read ceilings; structured operations tighten calls and share one read ledger across actor and clip owners. Cold block/index reads charge work and capacity before materialization. Complete list/scan refuses rather than returning partial coverage. Typed storage causes survive suppliers and host actor/job boundaries; I1 concerns the missing operation labels around those causes. |

The current source addresses the task reviews' earlier owner-type loss, graph-pair replacement, incomplete UI contributors, stale/frozen access and unbounded UI formatting findings. Those are not reissued here without contrary evidence.

## Qualification limits retained

- Bounds mean the specified stored-input/work and requested-capacity ledgers. They do not prove a global RSS ceiling, allocator/OS overhead bounds, compaction bounds, arbitrary external-writer safety, or power-loss durability. Separate clip opens retain their own recovery-open budgets; the shared query ledger covers reads, not aggregate recovery across all opens.
- The prepared recovery/read implementation deliberately refuses unsupported blob trees and optional byte-backed Slice behavior. Windows long/nonabsolute paths refuse. The recorded runtime environment is Linux/Rust 1.99.1; macOS and Windows runtime qualification and all future compiler/collection allocation behavior remain unproved.
- Actual actor recovery/refusal evidence is strongest for the unsealed session owner; capture/acknowledgement fixtures cover both failed owners. The entire per-owner/pre-unseal matrix is not exercised. The capture-library fixture explicitly invokes unavailable publication, so it does not independently prove every actor publication route. Shared query/replay exhaustion has attributable counters, but the failing owner is not isolated in every fixture.
- The release-source ruling on Fjall issue 332 remains appropriately narrow: v2 duplicate replay is charged, but the v3 FIFO poisoning mechanism is not demonstrated in this pinned v2/default-leveled path. No speculative backport is required by this review. The existing bounded corruption handling addresses the relevant strict-recovery concern without claiming general salvage.
- Browsing-history RecallIndex's separate empty fallback is outside the structured research completeness claim. M5 headed interaction, disabled-network cold reopen, real desktop/device behavior, and product-wide acceptance remain unverified here.

Resolve I1 and I2 in one host wave, preserve the failed receipt, and obtain a fresh passing full library receipt. Only a scoped check of that fix wave is needed unless it changes the contracts reviewed above. The two Minor gaps may remain documented deferrals.

## Scoped final-fix review, 2026-10-30

**Code verdict: Needs one narrow correction. I2 is closed; I1 remains open at one previously listed site.** Reviewed the complete 471-line `m4-final-fix.diff`, the final handoff in `m4-storage-host-brief.md`, and the relevant current receipt functions. The controller verified the 70-file `host-final-fix-freeze2-sources.json` freeze on HEAD `3113ef7f112e4da4ed3f321c60d9971488971368`. No tests/builds or source mutations were performed by this reviewer.

The source verification, preflight, deposit, sealed capture, inventory and recovery-emission changes restore operation labels by wrapping `SourceReadError`. The original typed cause remains reachable through `storage_error()`. The new real bounded-store test checks the source-manifest and recovery-receipt read labels together with exact `ValueBytes=0` causes. I found no new correctness defect in these changes.

**Remaining I1 location:** Turnstone `src/trail_memory/source_capture_receipts.rs:300`–`:301`, `acknowledge_saved`. Its receipt read still has only `.map_err(HostError::from)?`, so a read failure after the graph/facet save is published without the `capture save receipt read` context. This is the same acknowledgement site listed at line 260 in the original frozen review. The fix instead placed that label in `completed_legacy` at current lines 250–253, followed by its appropriate `legacy capture receipt unavailable` label at lines 254–257. The passing new fixture does not call `acknowledge_saved`, so it does not cover this omission.

**Correction:** move the `capture save receipt read` context to the read in `acknowledge_saved`; keep the legacy operation's own label. Extend the bounded `ValueBytes=0` fixture to assert the acknowledgement label and the unchanged exact storage cause. This is completion of I1, not a new broad review request. Preserve the in-flight gate's freeze and receipt before making the correction.

**I2 closed:** the refusal test separately requires `CancelResearchSearch` and `Redraw`, then rejects all other effects. The diff leaves all refusal/persistence/byte-preservation assertions and production cancellation intact.

The supplied post-fix receipts report 53 passing source-capture tests, one passing session-refusal test, and passing default/inference production-library checks. The full default library rerun `m4-host-storage-final-full-lib-fixed` was still running at this scoped review. It remains a separate validation gate, and any result applies to this 70-file freeze until a later correction is independently qualified. The two Minor deferrals and all M5/storage qualification boundaries above are unchanged.


## Controller closure of the scoped residual (verification pending)

The remaining I1 label was moved to `acknowledge_saved`; `completed_legacy`
retains its own context. The real bounded fixture now checks both the intended
acknowledgement label and the absence of that label on legacy reads, alongside
exact ValueBytes=0 causes. The controller inspected these sites directly.
The following full run had exposed two tests assuming Storage was the outer
variant; their exact kind/value assertions now use `storage_error()` through
the required context wrapper, retaining pre-unseal and replay-success checks.
This completes the prescribed I1 correction within the final fix closure;
covering and full-suite verification are still pending. No new whole-review
scope was opened and no passing whole-M4 claim is made here.


## Verified controller closure and publication

All 70 source hashes and exact status paths matched the final freeze3 manifest.
`m4-host-storage-final-full-lib-closure` passed 1,183 tests, zero failed and eight
ignored, exit 0 in 302.01 seconds, maximum RSS 339,208 KiB and zero process swaps.
Its raw log SHA256 is
`7c96888f7faf38b6b9b10f2cd880d99879f313c23c5af00c3ade7c61df02a9eb`.
The corrected source also passed 75 model-free, 53 source-capture and one refusal
test, plus production default/inference checks. Exact stored causes, labels and
cancellation protections remain asserted. I1 and I2 are closed based on the
scoped review, the controller's inspected prescribed correction and these fresh
gates; no additional broad review was run. The two Minor coverage gaps remain
documented deferrals. The exact committed blobs match the source freeze.
Turnstone `0089c9d331522343d81a69dcf1f3650ec662f4a4` was pushed after unchanged
upstream refresh and independently confirmed at remote main. M5 remains open.
