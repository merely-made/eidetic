# Final M4 cross-layer review brief

Review the completed M4 structured-query and bounded-storage change across Eidetic, Mere and Turnstone. This is the single broad final review, after task reviews and scoped fix loops. M5 headed/offline acceptance is a separate subsequent task, not claimed by M4. Check for real cross-layer errors and unaddressed task defects; avoid replaying resolved findings without contrary evidence.

## Authoritative scope

Canonical plan: ../../../design_docs/eidetic_docs/implementation_strategy/2026-10-11_eidetic_development_plan.md, P4a M4. User explicitly extended the milestone to bound storage reads and requested new-release investigation. Preserve exact source/version/hash/scalar-selector authority and current/historical distinction. Counts and absence require complete admitted input, never lexical top-k. Typed facts must be explicit and attributable, never inferred from arbitrary prose. Retain optional generation and mixed-source compatibility.

Structured authoring uses active Personae identity and existing Mere graph assertions. Canonical saved graph snapshots, including withdrawal tombstones, are durable authority; HostJournal is process-local, not durable event replay. Sealed pending receipts and sync fences prevent partially saved graph/store operations from entering results. Nonces identify authored assertions; exact repeated bindings collapse, different bindings sharing a nonce refuse. Saved results retain exact inputs, limits, query and historical lifecycle inspection; current-evidence reopening revalidates authority.

All three source owners (unsealed session, sealed session, retained clips) use bounded storage. One shared operation read ledger spans actor/worker calls and stores. Domain per-call limits reach the engine before materialization. Complete inventories must refuse rather than truncate. Typed limits must remain distinguishable from corruption/I/O/unsupported/missing. Explicit settings remain editable after owner refusal; saved changes activate after restart. Invalid/newer settings preserve bytes and require external repair.

The upstream-derived Fjall2.11.2 / lsm-tree2.10.4 patch preserves v2 retained format. Whole-open read-only admission precedes mutation; failed budget/corruption admission preserves preexisting files, with only a newly created coordination lock exempt. Common cooperative lease covers all updated opens and escaped capabilities. Temporary cleanup preserves root/lock inode. Limits cover named requested capacities and stored-input work, not process RSS, catchable OOM, cache/runtime infrastructure, background compaction, arbitrary external writers or power-loss qualification. Unsupported blob/bytes and Windows path cases refuse explicitly. Existence may traverse full range; cold reads bypass cache and optional Bloom filters are omitted in preparation.

## Review packages

m4-final-eidetic.diff: portable evaluator, sealed schemas, typed storage errors and Muniment facade, starting after M3. m4-final-fjall.diff and m4-final-lsm-tree.diff: current patch versus exact upstream crate versions, omitting unchanged vendored files. m4-final-mere.diff: only the two M4 supplier commits, omitting concurrent unrelated Mere work. m4-final-turnstone.diff: all M4 host source against current upstream, including untracked source. Read in manageable chunks. Controller must regenerate Turnstone package after task fix completion before dispatch.

Relevant task reports: m4-provider-review.md, m4-mere-bounds-review.md, m4-error-identity-review.md, m4-ui-review.md, m4-backend-review.md, m4-lsm-review.md, m4-recovery-review.md, m4-host-storage-review.md and their scoped fix appendices. Detailed gate provenance in canonical archive receipts. progress.md preserves rulings and failures; use it as context, not evidence that code is correct.

Deferred minors for final triage: Mere borrowed serialization parity fixtures omit optional-field combinations, without observed mismatch; core Error::source has no direct downcast test although typed getter propagation is tested. Supplier unchanged upstream warnings/whitespace are preserved; host-introduced warnings should be removed. Browsing-history RecallIndex has a separate stored_page_texts empty fallback; research snapshots do not use it, and its completeness is not qualified.

Release investigation: official Fjall3.1.13 fixes v3 duplicate journal replay/FIFO overlap (#332), but pinned v2 exact poisoning is not reproduced and v2 FIFO differs. No speculative replay backport; physical duplicate work is charged. New engine standard size_of/get/guards still materialize stored values, journal decoder allocates from lengths, official migrator iterates old values. Strict recovery issue311 and write-buffer issue344 remain open. Known malformed journal bytes now refuse rather than being treated as torn EOF tails. m4-release-replay-report.md documents source-level applicability and untested boundary.

## Reviewer procedure

Read-only review; no builds, no source edits, no subagents. Do not rerun suites already qualified on the frozen source. Inspect code outside provided diffs only to resolve a named concrete integration risk; describe that check. Return one consolidated report with Critical/Important/Minor findings, exact paths and lines, requirements covered/unverifiable, and approval verdict. Save m4-final-review.md here. Controller dispatch will add final source freezes and validation state. This draft brief alone does not assert that task fixes or M4 are complete.


## Additional task-review qualification boundaries

Host task re-review found two capture/ack owner-open failures were still
stringified, and loss of open/owner/restart guidance plus typed comparison-side
context. Round2 addresses those; final dispatch must name its reviewed freeze.
The host tests demonstrate shared source-to-record ledger exhaustion and exact
query/replay counter values but do not identify the exact owner where each
whole query/replay exhausts. The capture-library fixture exercises the real
refresh and explicitly invokes the actor's unavailable publication step; it is
not an automatic actor publication test. Whole-store supplier tests and a real
session-memory actor recovery test establish their own boundaries. A separate
real sealed-owner startup/retry and every owner/block/cumulative pre-unseal
combination are not all individually covered. Assess these as coverage limits
against actual cross-layer behavior; do not turn library tests into M5 claims.


## Host source freeze for final review preparation

Host round2 source: HEAD3113ef7f plus67 files in
`testing/model-free-m4-20261030/host-storage-review2-freeze2-sources.json`.
Focused73 tests and both production default/inference lib checks pass, all
locked/offline under the3GiB/zero-process-swap guard. Controller verified all
67 hashes. Actual actor tests now cover capture and acknowledgement events after
both real unsealed and sealed owner open-limit failures, preserving exact typed
cause. UI message tests check owner, Open field, numeric limit and restart;
typed comparison errors preserve side context. Round2 scoped re-review is
pending until controller dispatch says otherwise. Turnstone whole-M4 diff has
been regenerated at this freeze. Upstream remains7ca35b71; Eidetic094ef38 and
Merec103 are unchanged. Cargo.lock remains46120659...; exact async-trait promotion
changes dependency classification without lock churn.


## Final dispatch status (supersedes preparation notes above)

All M4 task reviews are approved, including host storage round3. Host final
source is HEAD3113ef7f plus68 files in
`testing/model-free-m4-20261030/host-storage-review3-freeze1-sources.json`;
controller independently verified all hashes. Focused74 tests pass and both
production library checks pass. Final full default library gate is running
separately; reviewer must not duplicate it. Host diff regenerated at this
exact freeze. Common retained-clip retention/import/export opens now preserve
typed cause until their String API boundary with accurate owner/Open/retry
message; targeted real retained fixture invokes all three. Source is held
through review. Supplier/Mere review and source revisions unchanged.
