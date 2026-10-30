# Eidetic development: content, encounters and computed memory

**Date:** 2026-10-11
**Status (2026-10-29):** active plan, owned by the independent Eidetic
repository. **Next milestone: [P4a, model-free retained research](#p4a-next-milestone-model-free-retained-research), M1 and M2 landed; M3–M5 open.**
The preceding producer and model receipts keep their bounded historical scope. The bounded custody connection and first producer-to-retention
paths below have published suppliers and locally qualified Turnstone integration.
Focused checks and bounded headed macOS receipt runs qualify saved-document
keyboard selection, sealed selected-text/voice receiving, cold playback and
onward export through a synthetic wallet-backed custodian. The Turnstone
consumer implementation and tested pins are published. The October 20
continuations also qualify embedded Knot primary-pointer selection and embedded
Redshank text-note retention, source-absent cold reopening and onward export.
These remain bounded native receipts over their recorded source cohort; the
text-note fixture supplies existing owner-model notes, rather than headed
full-owner authoring. Personal-vault migration and broader host gates remain
separate.
Turnstone's complete saved-source research workflow and authored Knot note
join have bounded macOS receipts, distinct from portable selected-content
transfer. The first Fleece portable-clip draft has
native transfer tests and browser compilation. Turnstone now qualifies native
selected text/WAV receiving, restart, local playback and onward copy; the broader phases
and remaining host checkpoints stay open. The first browser encounter slice
now has native regression, fresh-process disk and browser compilation receipts;
renewed headed browser intake and private custody remain open. P4's inference
surface is now qualified through the resident plan's I5 native gate; P4's other
embedding, assessment and tag-work requirements remain open.
The five gathering/intake dependency
moves are already complete in the
[Fleece and Eidetic checkpoint](https://github.com/merely-made/mere/blob/main/design_docs/archive_docs/2026-10-11_completed_plans/2026-10-11_fleece_eidetic_capability_plan.md).
**Scope:** develop Eidetic's retained-content and memory capabilities together
with Chartulary/Stemma, Muniment, Pandect, Inker, Fleece, ESP and lexical recall.
Graphshell and Turnstone supply the browsing consumers; Distillery supplies the
model-work consumer. Extend existing modules before proposing further crates.
The 2026-10-19 foundation promotion moved the portable core, Muniment,
Chartulary, Hagiograph, Stickleback, Luggage, proofs and Servitor to this
repository. Fleece, Pandect, Inker and application adapters keep their existing
homes. Lexical recall now lives in `crates/eidetic-search`. Its package tests
and optional recall example compile check pass on locked, offline Rust 1.99.1;
the model backends remain opt-in. Direct Mere and Turnstone lexical-source receiving is recorded in the October 26
progress entry; these package checks do not close model or product acceptance gates.
**Supersedes:** the
[June deferred-phases plan](https://github.com/merely-made/mere/blob/main/design_docs/archive_docs/2026-10-11_superseded_plans/2026-06-09_eidetic_deferred_phases_plan.md).
Its OPFS measurements and old index proposals remain historical evidence; their
remaining requirements are carried into P0, P3, P4 and P6 below.

## 1. Direction and ownership

Mark's direction in this conversation is the starting point:

- Fleece lets a person clip, crawl and capture content for a mere. Crawling
  includes local graphs and engine-produced content, directed by goals and
  queries. Ordinary browsing must not impose a crawl on encountered sites.
- Preserve individual visits and their relationships. Equal page addresses or
  equal captured bytes do not make two encounters the same event.
- Import is general intake into retained memory, extending beyond history
  import/export. Preserve original material and conversion evidence.
- Review Chartulary/Stemma, Pandect and Inker alongside Eidetic. ESP and Eidetic
  are reusable foundations that Distillery composes into useful model work,
  much as Turnstone composes graph and document capabilities.
- Make proper use of dependencies: examine their actual capabilities, our use
  of them, feature costs and target support as part of development.
- Selectively replicate collections of notes, clips, renderable documents and
  recognized structured exports from application runtimes into and across meres.
  A receiver should use retained content with the source applications stopped.

The ownership table is the proposed working model for implementation. It does
not authorize changing domain semantics or replacing stored formats silently.

| Owner | Responsibility | Consumer boundary |
|---|---|---|
| Muniment | Byte backends, immutable blobs, mutable slots, journals and atomic storage operations | The host chooses a backend and custody policy |
| Chartulary / Stemma | Resource graph, attributed edits, individual encounters, owner-scoped branches and their projections | External observations are admitted through explicit mappings; arena handles are not portable visit identities |
| Eidetic | Portable intake, typed retained content, schemas, source evidence, browsing corpus, model/corpus/evaluation artifacts | Keeps retained facts interpretable independently of an open browser or loaded model |
| Pandect | Session manifests and lifecycle, graph/reference adapters, settings, restoration and graph-aware retention | Session attachment and saved view state remain distinct from the durable domain's facts |
| Inker | Engine routing and the document/session/capture contracts engines implement | Reports what the selected engine can actually expose |
| Fleece | Extraction, clipping, observed content links and bounded traversal over supplied sources | Source acquisition and session admission remain explicit adapters |
| ESP / eidetic-search | Model execution, embeddings, numerical retrieval, lexical recall and ranking fusion | Results are derived from named source versions and settings |
| Distillery | Model selection, supervised jobs, execution status and reusable work surfaces | Uses ESP for execution and Eidetic for artifacts; the owning application admits proposed edits |

The dataspace draws on durable domain facts, current engine observations,
session context and derived computation. Its visible projection is not an
additional store of truth. Hagiograph remains the domain-defined history and
significance organ; ordinary browsing visits do not acquire significance merely
because they were recorded.

### Post-extraction stack review, 2026-10-20

This review separates the independent source home from the program's behavioral
gates. Eidetic is a family of headless memory, graph, distribution and admission
libraries; the `eidetic` crate is its durable-content member. Its normal
first-party dependencies use its own libraries and Dramatis identity primitives.
Murm is an Eidetic-core dev/carrier integration. Mere's shell, Fleece, session
composition and ESP execution are not production back edges of these libraries.
Fresh `rustup run 1.99.1 cargo metadata --locked --offline` confirms that the
resolved normal/build closure has no Mere application source: its external
first-party/fork sources are Dramatis and the patched p2panda source. This checks
layering in the workspace resolver configuration, not isolated feature costs or
browser execution.

The three principal compositions remain:

| Composition | Retained authority | Neighbor responsibility |
|---|---|---|
| Content and encounters | Muniment bytes/journals; Chartulary assertions and Stemma encounters; Eidetic manifests, typed content and evidence | Inker exposes document capabilities; Fleece extracts/assembles selected packages; Pandect and the host admit session attachments |
| Recall and model work | Eidetic source/passage/run artifacts; derived indexes cite those artifacts and versions | `eidetic-search` supplies BM25/fusion; ESP supplies execution/embeddings; Distillery/Djinn supervise and retain runs |
| Replication and acting participants | Stickleback accepted-operation storage/retention and Servitor scoped petitions/run reduction | Murm carries traffic, Gemot/domain policies authorize it, Dramatis supplies identity, and hosts execute admitted work |

Hagiograph's product-defined significance and Luggage's signed release/update
mechanics are additional independent consumers of this family's durable-history
and artifact concerns. They do not turn a visit into a significant event or a
content snapshot into an installable release.

**Published-source checkpoint.** Fetched main refs are Eidetic `738ef159`, Mere
`21deb0cb2`, Knot `aa81a33`, Woodshed `b2fe4df`, Isocosm `94d44212` and Turnstone
`a08444e`. The first four consumer root locks resolve the extracted packages at
one `738ef159` source each. Turnstone retains one `543f5ff3` source for its
qualified host cohort. Between those Eidetic revisions, changes are plan/docs
and Cargo feature inheritance; no Rust implementation changes. A pin difference
alone does not invalidate a frozen native receipt or justify a blanket relock.
The latest independent [library CI run](https://github.com/merely-made/eidetic/actions/runs/39381563909)
passes at `738ef159`: workspace tests and a no-default-features workspace check.
The promotion's 539-test receipt remains historical, not a new test count.
Primary checkouts may be older or dirty; this checkpoint uses published Git
objects, not their uncommitted state.

**Evidence and open boundaries.** Turnstone's
`docs/receipts/2026-10-20_knot_drag_retention/` and
`docs/receipts/2026-10-20_redshank_text_retention/` add native selection,
sealed retention, source removal, custodian/receiver restart and selected-only
onward export. The latter binds 36 focused cases, four shipping-process phases
and eight reviewed captures to `09a9ee68`; the source cohort remains explicit.
The custodian is synthetic and wallet-backed. Full-owner authoring, installed
personal custody, broader platforms and every P6 semantic checkpoint do not
follow from these passes.

Existing code already supplies `ClipTagV1` concept owner, statement and asserter
fields, Chartulary's assertion journal and the graph resource-tag adapter. The
new text receipt has empty tags; it does not qualify equal labels under distinct
owners, suggestion acceptance or a reviewed reconnect edit. Fleece's bounded
part traversal is implemented, while the native/browser heterogeneous collection
with required assets and preserved external relations remains a separate gate.
Muniment's Redb, directory and IndexedDB backends implement `transact`; Fjall and
ZIP refuse it. An IndexedDB implementation or browser compile pass is not a
headed browser durability/quota receipt. Deletion/reference policy and recovery
remain named P6 decisions.

**Recall re-entry cue.** In ESP `809deb6b` (the current Mere consumer pin),
`crates/esp/src/embed/persistence.rs` still stores numerical dimensions, metric
and entries without an embedding-space identity; `search.rs` checks dimensions
when attaching an index. P4 and resident I10 therefore remain a live shared
slice: persist model/algorithm, tokenizer/preprocessing, pooling/prefix/
normalization, metric, dimensions, chunker and source revisions; accept a matching
index after restart and refuse a same-dimension foreign space. Eidetic owns
retained evidence/schema, ESP owns provider/index compatibility, and the resident
recall consumer supplies the forcing end-to-end gate with lexical-versus-fused
measurement. Keep generic numerical indexes usable independently. This boundary
has higher shared value than another repository move or another equivalent
text-copy receipt. Attributed tag/suggestion qualification is the smaller next
extension when the immediate focus is portable-content semantics.

### Portable collections across meres

This is a concrete outcome of the program. A person selects content gathered by
Fleece or exported by a domain runtime; Eidetic retains its named format/version,
bytes, source evidence and required parts; the chosen mere admits a collection
with its meaningful membership and relationships. Another admitted mere can
replicate that selected collection and use supported content independently of
the original application. Local copying and peer delivery use the same content
contract, with their respective destination and disclosure authority.

Use the [shared semantic-core review](https://github.com/merely-made/mere/blob/main/design_docs/2026-08-12_family_composition_thesis_brief.md#shared-semantic-core-existing-contracts-and-missing-seams-2026-10-11)
as the starting inventory. Agree on a small versioned profile over existing
bundles, manifests, schemas and graph references. It must name selected roots,
payload roles, required material, selector units, provenance and the retained
relationships. Preview which assets and source evidence will actually travel;
following every graph edge or including an enclosing source document is not an
implicit export rule. Unselected or external references retain their identity
and availability state.

Format recognition, complete local bytes and supported operations are separate
capabilities. The family profile must have a tested receiving path for notes,
document clips and one recognized structured export. Genet or a registered
domain adapter supplies the actual render/inspect behavior. Additional formats
advertise their support and preserve unfamiliar material. An exported snapshot
retains its captured identity and meaning; editing, ongoing collection sync or
operating a live source needs its own declared owner contract.

### Knot/Redshank profile and owner adapters

Mark chose Knot Editor and Redshank as the first heterogeneous pair and handed
the shared profile, owner-adapter and tag design to this Eidetic/Fleece lane.
The four verbatim [clip policy rulings](https://github.com/merely-made/mere/blob/main/design_docs/2026-08-12_family_composition_thesis_brief.md#clip-policy-rulings-2026-10-11)
live once in the family brief: private source backing, note/body-first media,
generated tags as queryable suggestions, and reviewed queued edits. They inform
this draft. The schema remains a draft; the October 19 and 20 bounded native
receipts below qualify their specific producer/receiver paths.

#### First transfer draft (2026-10-11)

Implemented `eidetic::clip_profile` and Fleece's opt-in `portable-clip` feature.
The draft records distinguish logical/capture/source identity, selected parts,
native anchors, source facts, attributed tags and private backing. Raw body
parts use content-addressed Eidetic objects; built-in schema definitions are
available locally. Imports check the complete required inventory and hashes
before deposits. Existing identical objects keep their original custody and
sealing manifest. Store failure may leave verified immutable objects; application
admission and atomic transactions remain separate gates.

Knot's `knot-composition::selected_export` produces one copied text item, its
native document anchor, pack references and related tag labels. Author notes,
surrounding text and other collection items stay out. Fleece verifies supplied
complete-source backing against the anchor hash and original UTF-8 byte span,
and retains that backing separately. Projection recipes explicitly refuse this
plain-text adapter until a typed recipe exporter is qualified.

Redshank's `export_selected_note` exports one shareable native note with its
frozen recording target and representation receipt. Private notes refuse export.
Fleece requires supplied voice-body bytes and preserves millisecond units,
reaction-adjusted and press offsets, and unknown historical attribution. The
recording remains a reference; full recording backing and encoded excerpts
still need their owner adapter. A voice body is not silently promoted to a
copy of its referenced recording.

Export roots select only named retained parts and their declared dependency
closure. Traversal is bounded and cycle-aware; private backing is outside that
inventory. The package carries selected bytes and content IDs. It omits custody
manifests, seal markers and local custody provenance. Per-application limits
cover parts, bytes and metadata. These helpers use no network provider and
perform no session or graph admission. Foreign typed parts refuse this first
draft, while native selector/evidence vocabulary remains opaque source data.

Native owner-generated fixtures cross the serialized envelope, lose their
source store, and survive receiver-only Fjall close/reopen with exact text and
voice-body bytes. Tests cover missing/altered/extra/duplicate payloads, budget
and disclosure refusal, dependency cycles, stale/split-byte anchors, attribution
round-trip and existing receiver custody. A reversible sealer tests wiring only.
The browser check establishes compilation, not OPFS, rendering or playback.
The dependency check adds no external library: the enabled production edges
are Eidetic, Genet's extractor, Serde and Serde JSON. Eidetic remains built with
its existing default features disabled.

Turnstone's first receiving host now reads selected plain text and plays a
retained WAV body through its existing Inker/Redshank seams. Committed-source
native import, fresh-process restart with the package paths unavailable, and
missing-content refusal pass. Original recording bytes remain unavailable;
source selectors and unknown source-tag attribution stay distinct from playback.
That receipt uses an explicitly unsealed store and makes no encrypted-custody
claim. Its exact source, nine reviewed captures and limits are recorded in
`turnstone/docs/receipts/2026-10-11_retained_clip_receiver/README.md`
(Turnstone `bb09931`, implementation `d2fb07c`). Complete source-backing adapters,
browser receiving, additional assets/structured formats, signed source tags,
generated suggestions and reviewed reconnect queues remain under the host gate
below. This partial native receipt does not close that gate. See the
[Fleece fixture receipt](https://github.com/merely-made/mere/blob/main/crates/fleece/tests/fixtures/portable_clip/README.md)
for producer generation and qualification scope.

#### Native producer-to-retention continuation, 2026-10-19

The owner accepted the native receiving-host direction after a fresh source
assessment. Keep this plan as the shared contract; Turnstone's existing
`turnstone/design_docs/2026-10-11_retained_clip_receiver_plan.md` owns its host
actions and acceptance. Extend the existing APIs and one receiving path.

**Current baseline.** Turnstone `e3e94ce` joins saved passages to authored Knot
notes through Knot `ceaf41f`. Ordinary authoring, exact saved-source citations,
newer captures and offline cold return have a bounded macOS receipt at
`turnstone/docs/receipts/research_notes_20261018/README.md`. It records 1,077
workspace tests passing, 13 ignored, 12 reviewed captures and independent reads
of the exited source and evidence stores. R4's earlier receipt covers scoped
real-model Answers and both app and resident process restarts. Neither receipt
qualifies deliberate sharing or the broader Knot/Redshank portability gate.
The later foundation cutover is a separate dependency baseline: Turnstone
`9c8420d`, Knot `2a4a204`, Eidetic `de6b1dce` and Mere `4a7909be7` were the
receiving sources inspected here. Historical receipts retain their exact pins.

**Missing seams at the inspected baseline.** Knot composition's `selected_export` and Redshank
model's `export_selected_note` are owner APIs, but their inspected call sites
are examples/tests. Turnstone production imports prepared packages; its direct
Fleece owner-conversion calls are tests. Its newer Knot citation insertion is
a separate existing contract, which must preserve exact source evidence.
Fleece retains bounded selected parts and optional private backing, but its
current `check_record` refuses schema-bearing parts. Mere's shared
`document-lanes::retained_clip::read_store` accepts one plain-text or WAV body
and refuses additional required parts. Unknown selector/evidence values being
preserved does not establish unknown typed-schema support.

**Custody prerequisite at that baseline.** Turnstone imports and reads its retained
clip store with no `PayloadSealer`. Pandect supplies `WalletEpochSealer`, and
the resident keeper owns wallet epochs; the inspected Turnstone custody client
offers identity/signing and scoped derived keys, not the wallet-epoch adapter
needed by this path. Private source backing must use the existing custody
owner. Do not substitute a newly minted local key, silently omit available
backing, or expand the unsealed fixture receipt into a private-custody claim.
The foundation plan's custody-process design remains independently owned;
process extraction is not a prerequisite for using the current custodian.

**Implementation choice (2026-10-19).** The current custody route gains bounded
seal/unseal operations for the admitted Turnstone app. `content.retained_persona`
in Djinn's profile owner settings explicitly binds the wallet namespace; absent
configuration or missing epoch refuses, without creating a wallet or local key.
Each call carries the connected profile and expected public root. The custodian
holds its vault/profile guard through the bounded cryptographic operation, so a
lock or switch waits for an already admitted call; later calls recheck authority.
An application/profile/retained-clip scope joins the canonical content hash in
AEAD associated data. No epoch key crosses the application door. The first
operation accepts at most 512 KiB cleartext (plus the standard seal overhead),
within the existing 1 MiB native message limit. Larger source backing refuses.
The sealer loader must retain reads of every available historical wallet epoch.

Turnstone's production clip reads, imports, native retention and onward exports
use this adapter. Existing unsealed payloads cannot be silently reused as a new
private retention: they need explicit custody migration. Memory preflight checks
custody and supported presentation before the disk store opens. Source writes,
immutable deposits and receiving graph/session admission remain separate gates.
Redshank's host-blob playback adapter must decode supplied verified voice bytes
in memory; private clip playback must not produce the former plaintext WAV cache.

The native producer continuation captures the observed Knot selection, saved
source hash and document address before invoking the native composition export.
It rechecks that offer at execution. Redshank's standalone host offers a selected
note package only for shareable notes, resolves a voice body within its own
content-hash namespace, checks its bytes before output creation, and publishes
without replacing an existing export. The receiving host never acquires the
original recording to satisfy a voice-note body. The embedded Redshank host can
retain shareable text notes directly; a voice body absent from that owner must
arrive through the source owner's explicit package export.

Implementation order and done-conditions:

1. **Bind a real selection.** Join one explicitly selected Knot composition
   item and one shareable Redshank text note to their existing exporters and
   Fleece adapters. Capture owner/entity/revision evidence where the owner can
   establish it; keep runtime save/playback tokens out of durable identity.
   Done when ordinary host actions operate on the displayed selection, stale
   or changed destinations refuse, private Redshank notes refuse, and no
   acquisition or source write is implicit.
2. **Retain under the chosen custody.** Reuse the receiving host's authorized
   placement and current custody owner for selected content, available private
   backing and readback. Make supported retained parts, external recording
   references and unavailable operations inspectable. Done when real
   close/reopen resolves exact text, native selector units and provenance;
   locked/unavailable custody refuses private retention; and selection-only
   export excludes backing, custody metadata and unselected material.
3. **Qualify source-absent use and onward copy.** Stop source applications and
   origin services, remove package-path availability, restart the receiver and
   then copy the selection to an independent mere. Done when both receiving
   stores preserve declared identities, selectors and source evidence, no
   acquisition occurs, and failure/retry cannot acknowledge missing required
   bytes or duplicate admitted content. Record store writes separately from
   application graph/session admission.
4. **Add an owner-produced voice body.** Use Redshank's actual note and supplied
   audio bytes. Turnstone can play retained WAV bodies but explicitly lacks a
   microphone adapter; this work does not require adding recording there.
   Done when source-absent restart plays the held body and still reports an
   unavailable referenced recording honestly. Recording excerpts and acoustic
   quality keep their separate gates.

Signed source tags, accepted generated suggestions and reviewed reconnect edits
remain requirements of the whole host gate, with their own admission evidence.
Required images, a recognized structured export, browser receiving and peer
delivery follow the bounded native slice; they are not advertised by a
successful text/WAV transfer. This continuation changes no stored format or
public API by itself.

The draft profile describes meaningful portable material using existing Eidetic
bundles/manifests and graph identity. It does not serialize an application's
whole internal model or make every clip part a separate graph entity.

| Information carried | Required distinction |
|---|---|
| Logical clip or native entity; capture and retained revision | A byte hash identifies a representation, not the owner, clip identity or a live operation target |
| Stable part names and roles; media/schema versions | Selected body, note, visual, source backing, source reference and annotation have explicit roles |
| Selector basis, units and representation version | Knot UTF-8 byte spans, Fleece canonical-text code points and Redshank media time remain distinguishable; supported conversions carry evidence |
| Dependencies per promised capability | Locally retained, explicitly disclosed, required, optional and external material are distinct; dependency traversal is bounded and cycle-aware |
| Source/capture/derivative lineage; owner reference | Native source identity and known source revision survive export; unavailable or unknown facts remain explicit |
| Attributed tags and generated suggestions | Concept owner, tagging asserter and engine evidence remain separate; label-only views are not an interchange record |
| Current availability and supported operations | Read/render/inspect/play/annotate/edit need their own tested capability; a complete note need not include its referenced recording |

Retaining private backing and constructing a disclosed package are separate
operations over that profile. Fleece's full canonical text may exceed the
selection; an export must build an explicit selection envelope instead of
copying the annotation and all its backing by default. Producer adapters retain
the native selector and representation evidence. Generic receivers preserve
unknown parts and visibly report missing capabilities.

The disclosed selected part has its own versioned selector basis. Preserve the
native source anchor as lineage; any supported rebasing records the mapping
between those representations. Reading a selection offline does not establish
that the receiver possesses or can seek within the complete private source.

**Knot adapter.** Use retained composition and captured-artifact ports. Export
the selected body and source anchor; include available complete-source bytes as
private backing under CP1, and disclose only the requested material. Collection
organization is a separately authored revision with compare-and-set behavior.
The original item's signer is not automatically the author of later tags or
organization. Recover signed organization provenance when available; preserve
unknown historical tag authorship otherwise. Owner edits resolve the fresh
target and expected source revision through existing admission; an editable
document token is not the durable entity reference. Record partial retention
and authored-note outcomes separately where the owner does not promise atomicity.

**Redshank adapter.** Export the text/voice-note body and frozen recording target
first. A voice body requires its own audio bytes; the recording stays an explicit
reference or separately chosen backing/excerpt. Preserve representation receipts
and media-time units needed for qualified exact playback. Excerpt encoding and
source-time mapping require a separate explicit exporter. Current annotation
records do not establish historical authorship, user tags or a public source
revision. Add owner-issued revision/admission and durable outcomes at the owner
boundary, without promoting playback tokens or persistence generations to those
roles. Existing guarded seek behavior remains the starting point.

**Tag adapter.** Reuse identity-owned `TagConcept` and attributed resource
statements under the graph-semantics owner. Export the concept and exact tagging
assertion, including source/asserter/scope/time evidence, rather than a label set.
Personal tags apply to the retained clip or note. Source tags stay attributable
to their original target; whether to offer copying them onto a derivative is a
remaining policy choice. Suggested tags keep producer/run, input revision,
settings and review state outside asserted personal `taggedWith`; acceptance
creates a personal assertion citing the suggestion. Whole-resource tagging is
the first supported target; part tagging needs an explicitly qualified selector
profile. Classification records remain their own typed meaning.

**Reviewed reconnect edits.** The session/application owns the durable request
queue. Store supported inert intent, source-owner/entity identity, expected
source revision, payload schema, author and nonreused request identity. Keep
waiting-for-owner, needs-review, stale/denied/unsupported, submitted and durable
completion distinguishable. Reconnect resolves fresh ownership and offers;
review and current grants/revision checks precede submission. A saved scene
instance or session token cannot trigger automatic replay. Local annotations
have their own author and retention and remain available while the owner is away.
Queue budgets and expiry are configurable; missing source grants refuse submit.

Use Turnstone's existing `turnstone/src/knot_authoring.rs` and
`turnstone/src/redshank_host.rs` integrations as the first native receiving-host
candidate, rather than creating another host. Qualify Graphshell as the browser
receiver through its existing intake/session path. These are proposed forcing
hosts: current source integration does not establish collection admission,
portable media rendering or queued-edit support.

## 2. Findings, verified 2026-10-11

Review baseline: Mere `8601f7599e0eb59bd77b10899d49f8fc83792c90` and Turnstone
`1034fb64b133022926bb4c7135b2b3281d09cc3b`. Concurrent Forme implementation and
composition-doc work is present and remains outside this plan's changes.
These are source findings; older execution receipts are not renewed by this review.

1. [Stemma](https://github.com/merely-made/mere/blob/main/crates/eidetic/chartulary/src/stemma/mod.rs) already has
   distinct entry, visit and owner records, parent/child branches and projected
   edges. Extend that machinery instead of founding another visit store.
2. [ImportedHistoryVisitItem](https://github.com/merely-made/mere/blob/main/crates/eidetic/eidetic-core/src/import/mod.rs)
   carries a visit ID and optional external tab/window context, but lacks a
   referring visit ID. [history_event](https://github.com/merely-made/mere/blob/main/crates/eidetic/eidetic-core/src/import/history.rs)
   lowers to URL-based traces and second-derived timestamps without preserving
   the visit ID. The [Firefox exporter](https://github.com/merely-made/mere/blob/main/scripts/firefox_history_export.py)
   resolves `from_visit` to a URL and reduces microseconds to seconds.
3. The [extension](https://github.com/merely-made/mere/blob/main/ports/graphshell/web/extension/background.js) keeps
   a bounded visit queue and resolves parent IDs to URLs. Its live path chooses
   the latest visit returned for a URL, and can fall back to a history-item ID.
   [BrowserVisit](https://github.com/merely-made/mere/blob/main/ports/graphshell/src/capture.rs) has a visit ID but
   lacks profile-instance and parent-visit fields. Admission already stages
   access records, traces and graph updates before acknowledgement.
4. [Pandect](https://github.com/merely-made/mere/blob/main/crates/system/pandect/src/lib.rs) already separates
   session, graph, view, reservoir and wallet adapters. [Inker](https://github.com/merely-made/mere/blob/main/crates/inker/inker/src/lib.rs)
   reexports the engine-facing `document-session-api` contracts and owns
   `EngineDocument`. [Fleece](https://github.com/merely-made/mere/blob/main/crates/fleece/src/traversal.rs) already
   takes source/version references and emits observation receipts and omissions.
5. Generation has an exact [ModelSession](https://github.com/merely-made/mere/blob/main/crates/intel/esp/src/infer/session.rs)
   envelope. The [embedding provider](https://github.com/merely-made/mere/blob/main/crates/intel/esp/src/embed/provider.rs)
   exposes dimensions and metric; [persisted indexes](https://github.com/merely-made/mere/blob/main/crates/intel/esp/src/embed/persistence.rs)
   carry numerical shape without an equivalent model/preprocessing envelope.
   [with_index](https://github.com/merely-made/mere/blob/main/crates/intel/esp/src/embed/search.rs) checks dimensions.
6. Eidetic already retains [training corpora and evaluation artifacts](https://github.com/merely-made/mere/blob/main/crates/eidetic/eidetic-core/src/models/training.rs);
   [Distillery's trainer](https://github.com/merely-made/mere/blob/main/ports/distillery/src/trainer.rs) composes
   those with ESP. General inference hosting remains an open slice in the
   [Distillery models plan](https://github.com/merely-made/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-10-06_distillery_models_plan.md).
7. [Lexical recall](../../../crates/eidetic-search/Cargo.toml) uses the
   in-tree BM25 implementation and Genet's text segmentation, with no production
   Tantivy dependency. [Muniment](https://github.com/merely-made/mere/blob/main/crates/eidetic/muniment/Cargo.toml)
   already offers native backends and a target-gated IndexedDB backend. The June
   proposal for a separate Eidetic OPFS crate is not the current storage floor.

Additional collection review at Mere `d9d9b165f733979a7da563160b215f277181736b`:
[Bundle](https://github.com/merely-made/mere/blob/main/crates/eidetic/eidetic-core/src/bundle.rs) already names member
manifests, domain kinds and required flags. Its required-member verifier resolves
those direct members; it does not walk a recursive portable-content dependency
set. [BlobManifest](https://github.com/merely-made/mere/blob/main/crates/eidetic/eidetic-core/src/manifest.rs) already
records integrity, schema, source, provenance/privacy and trust metadata.
[Community collections](https://github.com/merely-made/mere/blob/main/crates/moot/gemot/src/moot/records/collection.rs)
retain causal membership over signed contribution references. That community
authority is distinct from an arbitrary private collection's selection/admission.
These mechanisms are the starting point, not proof of general cross-mere
content portability.

The handoff's adapter findings were checked at Mere `eac1524f`, Knot Editor
`4d8b81c5ed8fbe29bac9fc762cb71f161e00c784` and Woodshed
`9e4b37238d5febfcfe73b4bc9e291ec93b2ee1cd`:

- `knot-editor/crates/knot-composition/src/lib.rs` has a retained `CollectionItem`
  and `DocumentAnchor` with UTF-8 byte spans and optional complete-text hash.
  `knot-editor/crates/knot-composition/src/retention.rs` separates immutable
  retained material from mutable organization and its CAS revision.
- `knot-editor/crates/knot-editor/src/composition_retention.rs` returns the
  original writer and latest organization revision; the organization writer
  cannot be inferred from that original-author field. Its target/grant checks
  are existing owner machinery, not a general foreign-clip adapter.
- `woodshed/ports/redshank/model/src/lib.rs` has frozen timed targets, text/audio
  bodies and shareable annotation export. An audio body's identifier alone
  transfers no bytes. `woodshed/ports/redshank/desktop/src/diagnostics_worker_tests.rs`
  distinguishes persistence acknowledgement from prior model/UI state; those
  source tests are not re-run by this documentation review.
- [Resource tags](https://github.com/merely-made/mere/blob/main/crates/graph/graph-kernel/src/graph/resource_tags.rs)
  already have owner/label concepts and explicit imported asserters.
  [SemanticStatement](https://github.com/merely-made/mere/blob/main/crates/graph/graph-kernel/src/graph/edge_data.rs)
  retains statement identity, scope, provenance and time. Label projections
  discard that attribution and cannot substitute for an export envelope.

Browser API facts inform the adapter requirements: Chrome distinguishes
history items from individual `visitId`/`referringVisitId` records; `onVisited`
arrives before page loading completes. The importer must preserve individual
records and observe content separately. [Chrome history API](https://developer.chrome.com/docs/extensions/reference/api/history).
A normal page's History API concerns its own session; browser-wide intake needs
an authorized extension or exported data. [MDN History API](https://developer.mozilla.org/en-US/docs/Web/API/History_API).
Tab/document context can be enriched through separately admitted navigation
observations where supported. [Chrome webNavigation API](https://developer.chrome.com/docs/extensions/reference/api/webNavigation).
These API facts do not establish a current headed receipt for the proposed changes.

## 3. Dependency use is part of every phase

P0 establishes a small capability ledger. Each later phase updates its rows
with the actual API used, source/version, feature set, target, consumer and
validation. Dispositions are **use as supplied**, **adapt**, **repair our use**,
**upstream fix**, or **replace with evidence**. A dependency being available
does not make every feature useful; a custom implementation needs a measured
semantic or operational reason.

| Dependency family | Review in this program |
|---|---|
| petgraph, slotmap and existing Chartulary/Stemma | Use graph/arena facilities for their intended jobs; distinguish portable identity from local handles, and preserve resource and encounter semantics |
| Muniment over redb, Fjall and IndexedDB; web-sys bindings | Use transactions, scans, durability and platform errors that the backend actually supplies; keep filesystem/browser adapters below Eidetic |
| serde and existing codecs; BLAKE3, UUID and URL canonicalization | Preserve foreign identifiers and byte evidence; use established identity helpers and explicit schema/format versions |
| Eidetic Bundle/manifests/schemas, portable content references and existing collection records | Reuse member and integrity machinery; define a bounded content profile and its dependency traversal rather than replacing those mechanisms |
| Knot composition/capture ports, Redshank timed targets and graph tag statements | Adapt the existing owner contracts; preserve exact units, provenance, grants and durable outcomes rather than inventing parallel tag or owner authorities |
| jsonschema and existing Mere-native validation | Distinguish unavailable validation from invalid material; use the validator's supported behavior rather than duplicating its parser |
| Genet DOM/session/text contracts and Fleece extraction | Consume semantic documents, links, selectors and text segmentation already available; expose genuine capability gaps |
| Burn, CubeCL/wgpu, tokenizers and safetensors through ESP | Use provider/backend facilities, batch work and injected devices; preserve numerical and tokenizer receipts and the shared-device boundary |
| Armillary, mesh, Fetch and Iroh adapters | Reuse execution, cancellation, job and transport contracts; keep admission and resource policy with their existing owners |

Feature review starts with [Eidetic defaults](https://github.com/merely-made/mere/blob/main/crates/eidetic/eidetic-core/Cargo.toml)
and [ESP's optional persistence/model-session edge](https://github.com/merely-made/mere/blob/main/crates/intel/esp/Cargo.toml):
determine which consumers need signing and full JSON Schema validation. Separate
normal, build and dev dependency cones and test intended feature combinations
without accidental workspace feature unification. Do not claim dependency
counts or browser suitability from manifest inspection alone.

Version upgrades remain owned by the
[dependency currency plan](https://github.com/merely-made/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-10-08_dependency_currency_plan.md).
This plan supplies use cases and gates for its data/Meaning families, especially
Fjall storage, JSON Schema and tokenizers. It does not start competing lockfile
migrations. New records follow the [format rulings](https://github.com/merely-made/mere/blob/main/design_docs/2026-10-06_data_formats_brief.md);
existing signed, hashed and persisted bytes require explicit compatibility work.

## 4. Phases and done-conditions

### P0. Establish owners and dependency capabilities

Inventory the read/write paths of Graphshell capture, Turnstone navigation,
portable import, engine capture and Distillery model work. Classify repeated
types by meaning before consolidating them. Retain foreign transition tags
through deliberate mappings instead of assuming every enum is interchangeable.
Populate the ledger above from locked dependency source and actual consumers.

**Done when:** every planned write and derived projection has a named owner;
the ledger identifies existing APIs, unused useful capabilities, duplicated or
bypassed behavior, feature costs and target limitations. Each proposed shuffle
has a forcing consumer and gate. Missing source or runtime evidence is recorded
as pending rather than inferred. The first slice's wire and migration choices
are concrete and reviewable.
The portable-collection profile names its first producers and receiving hosts,
minimum supported formats and dependency/disclosure rules; schema validation
alone is not counted as a renderer or domain interpreter.

### P1. Preserve encounters through intake

Retain source instance/profile scope, original visit identity, parent reference,
transition, original timestamp precision and available tab/window/document
context. Keep capture-device context distinct from a source visit's known or
unknown original device. Preserve parents outside the selected import range,
and distinguish an unknown parent from an observed root. Handle out-of-order
records, conflicting source facts and branch claims Stemma cannot directly admit.
Raw relationship evidence remains inspectable when a branch cannot be resolved.

Browser adapters must distinguish history-item IDs from visit IDs and prevent
rapid revisits to one URL from collapsing through a latest-only query. Stable
source scope is required across imports; a new import-run ID is not a profile ID.
Map portable identities onto Stemma through a replayable admission mapping.
Review `TraceEvent`, the lineage projection and access records together so
lossless intake does not produce another independent encounter authority.

**Done when:** fixtures preserve repeated visits, two branches from one parent,
two profiles reusing the same visit number, parent arrival after child and a
parent outside the import range. Rapid live revisits survive capture. Re-import
and restart retain the same admitted identities without duplicating events or
moving a live navigation cursor. Existing stored records remain readable or
receive an explicit supported migration; missing facts are not fabricated.

### P2. Admit imported and live history into the chosen mere

Use Eidetic intake to retain source material, digest, scope and conversion
notices, including imports beyond browser history. Pandect and the application
session decide the destination mere and record accepted graph/history changes
through existing attributed funnels. Separate durable facts, session attachment,
navigation cursor and saved projection preferences. Counts and recent-page lists
derive from admitted encounters. Portable trace projections cite their sources.

**Done when:** imported and live encounters join one queryable history in the
selected mere and reopen through Pandect. A second session sees the same durable
facts with its own view/cursor. Retry after failure is idempotent, acknowledgements
follow successful durable admission, and destination switching cannot attach a
late result to another mere. Raw intake and notices remain inspectable under the
user's retention settings. Importing history does not fetch its destinations.

### P3. Capture engine content and relationship evidence

Connect Inker's actual document, clip and capture capabilities to Fleece's
source/version observations. Reuse the existing Fleece annotation and selector
contracts and keep raw material distinct from normalized extraction. Correlate
capture with the originating session and document version, using the
`turnstone/design_docs/2026-08-28_page_capture_plan.md` and page-lifecycle owner
for product attachment and stale-result behavior.

Retain separate evidence for followed navigation, a link contained in a captured
document, clipping provenance and a model-proposed relation. An available link
does not establish that the person noticed it, and an unclicked link is not
automatically a negative preference. An opaque engine must report its limitation.

Carry the completed gathering checkpoint's HTTP follow-through with Fetch:
the bounded client avoids a hidden pooled-request retry, but the existing
streaming checks do not establish a deterministic pooled-cancellation race
receipt. Qualify that race with a controlled negative case when the adapter
can exercise pool cancellation; keep it distinct from general traversal tests.

**Done when:** a loaded document and a non-web/engine-generated source both
yield retained content and inspectable relationships through the same Fleece
seam. Reopening resolves the captured version and selectors; stale completion
is refused. Gathering loaded links produces zero destination fetches. Explicit
crawling respects configurable node, depth, fan-out, request and byte limits,
cancellation, omissions and source receipts.
The HTTP adapter has a separately recorded controlled cancellation-race gate.
The selected runtime exporters also supply the collection profile's required
material and retain exact source/capture identity. A clip's export preview
distinguishes selected content, carried source evidence and external references.
Knot/Redshank producer gates apply CP1/CP2, distinguish unavailable recording
playback from a complete portable note, and keep private backing outside the
selected disclosure envelope.

### P4. Build recall and model work over named evidence

Give persisted embeddings a compatibility envelope naming the model/artifact or
deterministic algorithm, tokenizer/preprocessing, pooling/prefix/normalization,
dimensions, metric and source corpus revisions as applicable. Keep the generic
numerical index usable independently. Review existing lexical recall and ranking
fusion before choosing further indexing dependencies. Derived indexes are
rebuildable projections. Changed source versions invalidate claims that affected
results describe the current head; authorized historical evidence remains tied
to its exact retained version, as specified in P4a.

**Embedding continuation (2026-10-20): landed and qualified through resident
I10.** `models::embedding` owns the retained contract; ESP's opt-in guarded index
binds the provider's exact loaded bytes and transforms. Mere's separate native
processes accept the same private projection after cold reopen over retained
page text with the origin corpus absent, refuse foreign same-dimension spaces
and changed source/chunker/legacy records, and measure lexical/vector/fused
recall on a frozen corpus. Canonical metrics, hashes, commands and qualification
limits live in `mere/support/receipts/resident-embedding-20261020/qualification.json`
and Mere's resident inference plan, I10. This completes the envelope consumer
slice; P4's traversal, assessment/tag and review requirements remain distinct.

Compose one real goal/query consumer with Fleece traversal and ESP assessment.
Distillery's inference surface uses exact model sessions and retains input
references, settings, output references and terminal status in Eidetic. Cancelled
or partial work must remain distinguishable from completed work. Graph changes
remain proposals admitted by the owning application. The 2026-10-15 resident
inference I3 slice adds the `models::inference::InferenceRunRecord` binding for
question/output/checkpoint references, request and exact-session evidence,
distinct terminal/paused states, device visits and actual comparison provenance.
Distillery retains it in the encrypted checkpoint log. The 2026-10-16 I4
continuation adds `models::recall::InferencePassage`: exact source identity and
version, canonical text hash, Unicode selector, versioned chunker and supplied
text. Distillery seals those artifacts and fills the run's supplied passage refs;
its native trained-model gate is qualified against the pinned SmolLM2 row and
separately owned page-text storage. Source owners retain their
stores and current-version authority, and current citations fail closed on
staleness. Derived tag proposals remain separate. This is the evidence side
of the Distillery plan's M2/M3, not another execution supervisor. I5 now
qualifies the Turnstone consumer: retained text stays behind its existing
trail actor, the resident seals supplied passages, and a separate native
Turnstone process streams, pauses/resumes and cancels real GPU work. After
both processes restart with the origin offline, the pane reopens the past
answer and exact source; sealed run and prompt evidence remain unchanged.
The resident inference plan's I5 receipt records this inference-surface
qualification. The other P4 requirements below remain open.

**Done when:** restart accepts a matching embedding envelope and rejects a
same-dimension index from another space. A real lexical/model provider produces
query results over retained visits and captured content, cites source versions
and explains traversal choices. One reusable Distillery surface streams a real
run, cancels it and reopens its retained record. A meaningful recall/latency
comparison measures the selected dependency use; stub-provider success alone
does not qualify this phase.
**Bounded answer continuation (2026-10-22, implementation).**
`models::answer::AnswerSupport` binds model-declared claims to exact supplied
passage identities and canonical Unicode quotation ranges, with the raw output
hash, reproducible structural issues and explicit unreviewed semantic state.
Distillery retains the typed artifact through its existing sealed journal and
Turnstone exposes the opt-in evidence answer and source inspection. Insufficient
and conflicting are model declarations, not proofs of absence or contradiction.
Malformed output, invented quotations and repeated claims remain recoverable.
The library contract has focused coverage; model and headed/cold qualification
are tracked separately in Turnstone's `cross_source_answers_20261022` packet.

Generated tag suggestions are queryable with their run/input evidence. Accepting
one creates a personal assertion; rejecting or regenerating suggestions cannot
rewrite existing personal tags. CP3's evidence and review state survive reopening.
Runtime observations, retained clips/notes and rebuildable indexes keep separate
lifecycles. A suggestion can be queryable at its declared keeping tier without
becoming a personal tag or requiring an explicit clip-promotion operation.

### P4a. Next milestone: model-free retained research

**Status (2026-10-29): M1 and M2 landed; M3–M5 open.**
This is the next Eidetic milestone. It advances P4's evidence and retrieval
capabilities independently of model execution; it does not close the remaining
P4 model, assessment/tag, P5 browser or P6 custody/exchange gates.

**Goal:** with no model files present and network access disabled, a person can
retain captures, selections and notes, restart Turnstone, search their admitted
material, reopen exact cited versions, compare revisions, and answer a documented
set of structured questions with inspectable inputs.

**Architecture:** Eidetic owns portable records and evidence; `eidetic-search`
owns derived retrieval, comparison and query evaluation. Turnstone composes its
existing source owners and presents the results. Distillery consumes the same
retrieval primitives for optional inference, while keeping model jobs, prompts,
embedding providers and model-authored reviews in their existing owners.

**Tech stack:** existing Rust 1.99.1 workspaces, Eidetic typed/sealed storage,
Chartulary attribution and facets, the in-tree BM25/tokenizer, and Turnstone's
existing Actions, research bridge, saved-source reader and Cambium surfaces.
No new database, query-language runtime or execution supervisor is required.

**Spec and decision:** the goal and behavioral contract in this subsection are
the implementation spec. **eidetic_development D1 (2026-10-26, new):** Mark:
"Yes, let's make that the next Eidetic milestone. Write up the model-free baseline
plan, grounded in the current code and the cross-source work we should keep. be
sure to check upstream first!" The milestone follows the preceding agreed
model-free baseline. The API names, phase boundaries and initial query profile
below are planning choices, not additional quoted rulings.
Links: area recall · plans P4a · summary Model-free retained research is the next milestone.

For implementation, use `superpowers:executing-plans` or the user's chosen
execution workflow task by task. Checkboxes below remain open until their
specific deliverables and checks pass. Next is M3 version comparison; M4 structured
queries and M5 product/restart qualification remain open.

#### Findings: upstream basis and retained work, 2026-10-26

The integrated cross-source foundation remains the basis of this milestone.
Prior source snapshots and software evidence are recorded in the
[receipt archive](../../archive_docs/2026-10-29/2026-10-11_eidetic_development_plan_receipts.md).

- `crates/eidetic-search/src/{document,index,candidates,tokenize,fusion}.rs`
  supplies weighted BM25, exact-address lookup, prefix candidates, reports and
  optional ranking fusion. Its normal lexical dependencies require no embedding
  runtime; ESP remains an optional/example/test edge.
- Both rewritten cross-source answer branches were integrated into their
  consumer mains. Preserve that history and its original successful and failed
  packets. Existing software receipts do not qualify a model-free product path.
- Keep `crates/eidetic-core/src/models/recall.rs`: source identity, immutable
  version, full canonical-text hash, half-open Unicode-scalar selectors and
  content-addressed passages. Keep the existing schema bytes and passage IDs.
  Their historical `models` path is not a reason to rewrite stored records.
- Keep cross-source scoped admission and source reopening in
  `turnstone/src/research.rs`, `src/retained_clip.rs`,
  `src/trail_memory/saved_sources.rs`, `src/app/contributed_pane_arms.rs` and
  `src/shell/saved_sources.rs`. Captures and sealed retained text already have
  concrete admission and exact-reader paths. Selections and notes can share
  a text representation without becoming the same producer kind.
- `turnstone/src/saved_sources.rs::search` already works without a model, but
  covers admitted captures using exact-case substring search. It is not the
  unified, ranked three-source path. Extend its consumer rather than adding
  another independently maintained saved-source browser.
- `mere/ports/distillery/src/inference/recall.rs::RecallIndex` supplies chunking,
  scoped rankings, current-head observations and content-before-identity score
  tie ordering. M1 now delegates the lexical part to `eidetic-search::recall`.
  The optional generation callers remain under `inference-model`, and
  `ask_internal_planned` requires a durable model session.
  `inference_protocol/recall.rs` is runtime-free wire data, but its
  generation contract and four-passage defaults are not a search product API.
- Keep `models/{answer,requested}.rs`, Distillery's
  `inference/{answer_contract,candidate_review}.rs`, sealed run journals and
  Turnstone's claim/source inspection for optional generation. Requested-fact
  spans identify parts of a question; they do not implement typed query parsing.
  `SemanticReview::Unreviewed` must remain so for existing model judgments.
- Keep the original successful and failed packets, especially Turnstone's
  `quoted_review_20261024`, `semantic_challenge_20261026` and integration receipt.
  The challenge records 8/15 semantic passes despite 15/15 structural passes.
  It used synthetic PageText sources and does not qualify native producer intake.
  Neither that failure nor the four-case success is a verdict on all models.

#### Findings: current receiving and workshop boundary, 2026-10-29

- Refresh upstream and inspect the resolved type-bearing dependency graph when
  receiving the implementation. The supplier's source identity and the consumer
  patch tables must agree; preserve unrelated locked dependencies.
- Alembic now owns shared selected-source proposals, review and acceptance
  (`mere/crates/distillery/alembic/src/workshop.rs` and
  `turnstone/src/alembic_workshop.rs`). Ordinary search stays directly callable;
  creating a durable draft can reuse that workshop. Do not build a second review
  and acceptance surface or require an agent job for every search.
- Alembic source selectors use UTF-8 byte offsets. Eidetic passage selectors use
  Unicode scalar offsets. Any M2 handoff must convert against the exact retained
  bytes with boundary checks; neither serialized contract changes.
- The workshop's bounded selected-passage brief is an adjacent capability. Its
  private text limit and selected-source count are not search corpus limits.
  Its capture fixtures do not replace the three real producer kinds or fresh
  offline restart required by M5.
- Scene responsiveness and synchronous owner journal writes remain a separate
  Turnstone investigation. Follow the existing owner queue; detached saving must
  not overwrite a newer graph snapshot. This is not a prerequisite for extracting
  the portable retrieval code.
- M1 keeps `InferencePassage` and the existing wire types. Its shared API needs
  only an owner, explicit source IDs, admission revision and character bounds;
  neutral operation envelopes, rich query modes and saved operation records
  remain M2–M4 work. Low-level ranking over supplied head observations does not
  itself grant authority. The public search operation rechecks admission and
  heads; legacy generation retains its existing evidence-assessment boundary.

- The initial Turnstone library-test build exceeded the compiler memory budget.
  After receiving the concurrent optional-service/source split, the cold build
  reached its time limit; a warm bounded run passed the saved-source and research
  suites. The guard stayed unchanged. Keep those earlier limits in their receipts;
  neither focused software tests nor compilation qualify native restart.

- Turnstone's concurrent upstream composition cut keeps `research`,
  `retained_clip` and `saved_sources` in the default build while gating the live
  inference service and surface behind `inference`. Preserve that split in M2;
  ordinary search must remain in the default product. Its new source-size and
  selected-feature dependency boundary checks apply to subsequent work.

#### Behavioral contract and global constraints

1. **One ordinary path.** Search, inspect, compare and structured-query controls
   work without connecting to an inference resident, locating model weights or
   acquiring a device for model execution. Turnstone retains its ordinary shared
   graphics device for rendering. Existing custody access can still require unlocking. Model
   actions are explicit additions; absence of a model cannot disable research.
2. **Three source kinds.** The first native gate uses a retained document capture,
   a real selected-text clip, and a retained note. Show actual producer metadata;
   do not infer a note from arbitrary prose or from `RecallSourceKind::Fleece`.
   Preserve selected-only disclosure and distinguish a retained snapshot from
   its originating live application document.
3. **Authority precedes computation.** Scope is an explicit admitted set plus
   its session/admission generation. Empty selection means no selected sources
   at the product boundary. An explicit “all admitted sources” control enumerates
   that set; it does not inherit the old empty-slice-means-all convention.
   Recheck admission before returning results and before reopening evidence.
4. **Exact evidence and historical access.** Search normalization never changes
   canonical text or quotation offsets. An authorized old retained version is
   valid historical evidence even when a newer head exists. “Current”, “older
   version”, “head unknown”, “unavailable” and “withdrawn from scope” are separate
   states. Missing old bytes cannot be substituted with a current page fetch.
   This historical inspection path does not weaken the existing generated-answer
   route's requirement for current admitted inputs.
5. **Bounded absence.** Search reports “No passages matched this query in this
   scope.” Structured queries report “No matching records in this scope” only
   after a complete enumeration of the selected supported records. Unavailable,
   refused or limit-exceeded inputs produce incomplete/refused results, never
   a zero count or an absence claim. Result pagination is distinct from corpus
   truncation. A top-k lexical shortlist is never the domain of a count or sum.
6. **Reproducibility.** Retain operation/version, resolved query, scope and input
   revisions, tokenizer/chunker or property-schema identity, settings, output
   and exclusions. Replaying frozen inputs reproduces the result; rerunning
   against current sources is a separate action with a new input snapshot.
   Existing owners retain sealed bytes; projections remain disposable.
7. **Small structured vocabulary.** First support source lookup, counts by kind
   or source, capture/record timelines, explicit tag membership, typed property
   lookup, grouping equal values, comparison and sum of compatible numeric
   records. Joins use explicit entity/source IDs. Date ranges, status and unit
   filters are explicit controls. Free-form questions continue to return lexical
   evidence unless the user selects a supported structured operation.
8. **Typed assertions.** Reuse Chartulary's attribution/withdrawal journal and
   Eidetic typed payloads. A property record binds an explicit entity ID,
   namespaced property/schema revision, typed value, optional valid-time interval,
   explicit status, author/method and evidence reference. Record time and valid
   time stay distinct. Unknown time/status is unknown, never implicitly current
   or approved. Evidence is an exact passage or a typed field in a named record;
   a producer timestamp does not need an invented prose quotation.
9. **Comparison rules.** Property definitions specify single/multiple values,
   value type and units. Different values are a disagreement only within the
   same entity/property/context, overlapping known valid time and comparable
   status. Otherwise present distinct or non-comparable values. Preserve all
   references when grouping equality. Copied text and multiple references do
   not imply independent corroboration. Never infer common entity identity from
   matching names. Never infer a version chain from a shared URL alone.
10. **Configurable bounds.** Expose search weights, result count, snippet context,
    corpus/operation limits and alias expansion through existing host settings;
    retain their effective values. Use current safe bounds as named defaults
    where applicable, not an unexplained generation-context cap. Limits refuse
    or report incomplete work explicitly. Keep current storage, custody, deletion
    and transport policies; this milestone does not grant new access or retention.

Advanced fuzzy matching, general Boolean/proximity syntax, automatic prose fact
extraction, extractive briefs and new federated search are follow-ons. Baseline
search includes ranked terms, exact phrases, exact addresses, explicit prefix
mode and metadata filters. A user-maintained alias list may expand terms only
with the expansion visible; it never establishes entity equality. The useful
baseline does not wait for general NLP, TextRank, SQL or SPARQL adoption.

#### File ownership and proposed interfaces

Extend existing crates; create focused modules, not a new foundation crate.
Paths here are relative to their named repositories. New APIs below are
**illustrative signatures to implement**, not existing or compile-ready code.

| Owner/files | Change |
|---|---|
| Eidetic `crates/eidetic-core/src/recall.rs` (new), `src/lib.rs`, existing `src/models/recall.rs` | Neutral operation/evidence envelopes using the existing source/passage types; retain their serialized identity. |
| Eidetic `crates/eidetic-core/src/records.rs` (new), existing `typed.rs`, Chartulary `assertion.rs` and `facet.rs` | Typed property/profile and operation-result schemas; use existing sealed save/load and attributed lifecycle, without a second fact database. |
| Eidetic `crates/eidetic-search/src/{recall,query,compare,records}.rs` (new), `lib.rs`, existing `document.rs`/`tokenize.rs` | Shared retained-text retrieval, explicit query evaluation, text comparison and structured evaluation. |
| Mere `ports/distillery/src/inference/recall.rs`, `inference/recall_embedding.rs`, `inference_protocol/recall.rs`, relevant Cargo manifests | Delegate lexical work to Eidetic; keep embeddings, prompt assembly, generated-answer validation and existing wire formats in Distillery. |
| Turnstone `src/research.rs`, `saved_sources.rs`, `inference_service.rs`, `app/{saved_source_arms,contributed_pane_arms,research_note_arms}.rs` | One admitted retained-source adapter; reuse exact capture/clip readers and note ownership; separate it from model connection state. |
| Turnstone `src/research_service.rs`, `research_surface.rs` (new), existing `inference_surface.rs`, `action.rs`, settings and `shell/drive.rs` | Model-free operations/state and presentation integrated into the existing research surface; optional generated-answer view keeps its existing pane/source serialization. |

Richer operation boundary for M2–M4 (illustrative). M1 uses the smaller
`eidetic_search::recall::{RetainedSourceOwner, RecallIndex, RecallIndexConfig}`
boundary; exact reading and saved operation envelopes remain later work.


```rust
// Illustrative; operation structs are specified below.
trait RetainedSourceOwner: Send + Sync {
    fn snapshot(&self, scope: &ResearchScope) -> Result<ResearchSnapshot, ResearchError>;
    fn read_exact(&self, key: &SourceVersion) -> Result<RetainedSourceText, ResearchError>;
    fn observe(&self, scope: &ResearchScope) -> Result<SourceObservations, ResearchError>;
}
fn search(snapshot: &ResearchSnapshot, query: &SearchQuery,
          settings: &SearchSettings) -> Result<SearchResult, ResearchError>;
fn compare(left: &RetainedSourceText, right: &RetainedSourceText,
           settings: &CompareSettings) -> Result<Comparison, ResearchError>;
fn evaluate(snapshot: &RecordSnapshot, query: &RecordQuery)
    -> Result<RecordResult, ResearchError>;
```

`ResearchScope` carries the owner/session generation and explicit source IDs;
`SourceVersion` pairs a `RecallSourceId` with its exact version. `ResearchSnapshot`
holds admitted texts, producer metadata, observations and enumeration coverage.
`SourceObservations` records current head and access state separately.
`SearchQuery` is one of ranked terms, exact phrase, exact address or prefix,
with kind/source/date/tag filters and resolved alias expansion. `SearchResult`
carries ranked exact passages, match ranges, pagination and coverage.
`Comparison` carries both version/hash identities and exact unchanged/inserted/
removed scalar ranges, or an explicit limit refusal.

`PropertyDefinitionV1` fixes property ID/revision, type, unit and cardinality.
`PropertyRecordV1` carries the assertion fields in constraint 8 and links its
lifecycle to the existing owner journal. Initially accept text, boolean, integer,
fixed-scale decimal with unit, civil date and offset-qualified timestamp; reject
non-finite, ambiguous or incompatible values instead of coercing them.
`RecordSnapshot` contains admitted property/metadata records, their schema
revisions, lifecycle observations and coverage. `RecordQuery` enumerates the
operations in constraint 7; property definitions also enumerate supported status
IDs, and tag filters use concept/owner identity rather than label equality; `RecordResult` retains every contributing record ID,
operation parameters, grouping keys, result and completeness. Arithmetic checks
overflow and units; undated rows stay visible in an unknown-time group.
`ResearchError` distinguishes denied/withdrawn, unavailable, invalid, unsupported,
changed-during-operation and limit-exceeded. Existing host adapters can translate
these values without silently collapsing them into an empty result.

Saved operation records contain the exact inputs or durable references to them,
including full-set identity for aggregate queries. If policy later removes an
input, the saved result can be displayed as historical but cannot claim fresh
verification; reopening or replay reports which input is unavailable.

#### Review focus

- A scope/session changes during work: old results must not appear in the new
  session or disclose removed text (M1/M2).
- Emoji, combining marks and phrase matches across chunk boundaries: the exact
  retained quote must reopen without byte/scalar confusion (M1/M2).
- A source is superseded, withdrawn or physically missing: distinguish those
  outcomes while preserving authorized old evidence (M3/M5).
- Repeated sources, unknown times/statuses, different units and multi-valued
  properties: avoid double counts and invented disagreements (M4).
- A corpus exceeds a bound or an owner refuses enumeration: show incomplete work
  rather than authoritative absence or totals (M2/M4/M5).

#### M1. Extract and receive the model-free retrieval boundary

**Files/interfaces:** `eidetic-search::recall` and Distillery files above;
produce `RetainedSourceOwner`, `RecallIndex` and `search`, while preserving existing
`InferencePassage`, model protocol and passage IDs.

- [x] Add focused `model_free_` tests for exact scope, empty selection, head
  observations, stable content-first ties, duplicate source/version handling,
  Unicode selectors and admission changes. Assert the same retained source
  bytes produce unchanged passage IDs before/after the extraction.
- [x] Run the focused tests and record the missing-boundary failure; extract the
  lexical code once. Keep model-specific `prepare` and `RecallEvidence` assessment
  in Distillery; adapt its corpus and embedding path to the shared result.
- [x] Receive one coherent provider revision through Mere and Turnstone. Inspect
  the resolved normal/build feature tree: the shared baseline does not activate
  `esp/bert`, `esp/decoder-flex`, `esp/model-session` or a GPU backend. Other
  unrelated application features are not evidence of a retrieval dependency.
- [x] Run the focused shared tests, the existing Distillery recall tests and
  provider no-default-features check using the commands below; commit the
  passing source cut and explicit receiving pins. Keep historical JSON readable.

**Done when:** a model-free test caller retrieves cited passages through the
shared API, and the existing inference caller uses that same lexical code.

#### Progress, 2026-10-29

- M1 provider, Distillery adapter and Turnstone receiving landed. Shared tests,
  legacy lexical/embedding recall checks and native/Wasm provider checks pass.
  Turnstone's rebased saved-source/research tests and optional-inference compile
  check pass under the unchanged guard. Evidence and prior failed attempts live in the linked receipt archive.
  M2 now supplies ordinary three-source search and exact inspection, as recorded below.

#### M2. Search all three retained source kinds in Turnstone

**Findings (2026-10-29):** upstream refreshed before implementation and again
before publication to receive the custody/projection split. The retained clip
reader now validates the complete text hash and Unicode-scalar selector, holds
its activation custody epoch and displays the exact quotation. Retained clip
date metadata is not exposed by the portable clip body; unknown dates stay
unknown. Existing capture enumeration and retained-clip owner queues remain the
authority for the default-build search path. Admission and custody invalidation
must run at both action and update boundaries, including when settings refuse
a search. Producer classification preserves edited selection bodies and actual
Knot/Redshank profiles; ambiguous profiles remain explicitly unclassified.

**Files/interfaces:** `crates/eidetic-search/src/query.rs`, existing
`document.rs`/`tokenize.rs`, Turnstone `src/research_search.rs`,
`src/app/research_search_*.rs`, `src/app/research_clip_arms.rs` and research
owner adapters. Consume M1;
produce searchable admitted snapshots and exact clickable results.

- [x] Add `model_free_` library cases for capture, selection and note in one
  admitted set, source/type/date/tag filters, phrases crossing chunk boundaries,
  explicit prefix and alias modes, metadata-only matches, pagination and
  incomplete enumeration. A metadata-only hit must not fabricate a body quote.
- [x] Run the new tests to establish failure, then implement the query modes
  over the existing index and exact canonical text. Evaluate phrases against
  complete admitted text before extracting snippets; never join across sources.
- [x] Extend the existing source chooser/search flow with mode and filter
  controls. Reuse exact reader actions and owner-scoped asynchronous work;
  cancel/discard results on admission-generation changes. Show effective scope,
  expansion and coverage. Register Actions/semantics once for UI and automation.
- [x] Run focused shared and Turnstone tests, including the existing saved-reader
  and research suites; commit the unified path and retire duplicated search
  logic only after its behavior is represented by the shared implementation.

**Done when:** all three kinds can be searched and inspected with no inference
service connection, including exact source/title-only and Unicode controls.

**Progress (2026-10-29):** the shared query provider landed with explicit modes,
filters, optional exact passages, stable pages and bounded frozen-input records.
Turnstone now uses the existing Saved Sources surface for all three producer
kinds, with persistent settings, explicit scope, filters, aliases, pagination
and coverage. Its coalesced owner work discards stale results and keeps private
search text out of ordinary URL navigation and history. The capture-only search
loop is retired; exact readers and the quiet workshop source loader remain.
Shared regression/Wasm checks, rebased Turnstone research and reader tests,
optional-inference compilation, dependency boundaries and source-size checks
pass. The default search path requires no inference service. Headed product use
and fresh offline restart remain unqualified until M5. M3 comparison is next.

#### M3. Compare exact retained versions

**Files/interfaces:** shared `compare.rs`, Turnstone service/surface and exact
source readers. Consume M2's exact-source reader; produce `Comparison`.

- [ ] Add `model_free_` cases for equal text, insertion/deletion, repeated lines,
  Unicode ranges, old/current versions, missing old bytes and a limit refusal.
  Assert comparison has both version/hash IDs and reconstructs each input from
  the reported spans. Different source IDs require an explicit two-source
  comparison, not an inferred revision relationship.
- [ ] Run the failures, then implement a bounded deterministic line diff with
  scalar ranges into canonical text; use hashes for exact equality. Show old
  and new sides with their own citations, and distinguish an explicit comparison
  from a source-owner-declared version sequence.
- [ ] Run shared comparison and Turnstone open/withdrawal controls; commit.

**Done when:** an authorized older snapshot opens and compares offline, while
withdrawn or missing material produces its precise state rather than a fetch.

#### M4. Answer the supported structured questions

**Files/interfaces:** core `records.rs`, shared search `records.rs`, existing
sealed typed storage and owner attribution; Turnstone research service/surface
and annotation actions. Produce the property schemas, snapshots, `RecordQuery`
and `evaluate` described above. No arbitrary-prose extractor enters this task.

- [ ] Add `model_free_` cases for source counts/timelines/tags, typed lookup,
  equality grouping, explicit-ID joins, comparisons and checked sums. Include
  two records of 09:30, a comparable 10:15, a former interval, unknown timezone,
  guessed versus approved status, two same-named entities, copied sources,
  multi-valued properties, incompatible units and arithmetic overflow.
- [ ] Assert that all contributing IDs and settings survive sealed save/load;
  duplicate projection of one assertion counts once, while distinct assertions
  remain individually inspectable. Assert incomplete input refuses totals and
  absence claims even when a lexical shortlist happens to contain all matches.
- [ ] Run the failing tests; implement metadata adapters and explicit typed
  annotation authoring through the existing graph owner, schema validator,
  journal and sealed payload storage. Withdrawal updates the existing lifecycle;
  derived query indexes never become the authority. Validate references and
  scope again when a query runs or a typed annotation is committed.
- [ ] Implement structured query controls and templated result sentences/tables,
  with visible input records and schema/status/time/unit filters. Unsupported
  free text returns passages and the supported-query choices, not a guessed
  structured answer. Preserve unknown values instead of defaulting them.
- [ ] Run core/storage, shared evaluator and Turnstone focused tests; commit.

**Done when:** a user can enter an explicit sourced property, reopen it, run
these queries, and inspect the exact records and operations behind each result.
The ferry fixture demonstrates typed comparison, not general ferry prose reading.

#### M5. Qualify offline restart and ordinary product use

**Files:** new Turnstone `scenarios/model_free_research_{seed,reopen}.scn`,
`scripts/verify_model_free_research.py`, existing `src/scenario.rs` and
`src/shell/drive.rs`; receipts under
`turnstone/docs/receipts/model_free_research_20261026/`. The script validates a
receipt from a run; it cannot stand in for performing that run.

- [ ] Extend existing real producer fixtures to retain a capture, actual selected
  text and an authored note through their ordinary controls. Record producer
  provenance and source boundaries; do not substitute three synthetic PageText
  rows. Preserve full-source backing privacy during selection and onward export.
- [ ] Start from isolated model-free stores/configuration, record that model
  directories are absent, disable acquisition/network access, and stop the
  inference resident while retaining the required existing custody service.
  Record process IDs, exact executable/lock/source hashes and attempted network
  acquisition. Never remove the user's models or personal stores for this gate.
- [ ] Exercise search, exact source opening, comparison, typed annotation and
  every structured operation. Save and exit. Remove original fixture sources
  and restart Turnstone and its test custody process; reopen retained sources
  and saved results. Show old/current, missing, withdrawn, scope-switch,
  model-unavailable and over-limit controls through the same UI/actions.
- [ ] Require reviewed headed evidence and independent byte/record assertions:
  exact quotations and selected-only retention, replayed totals/groups,
  unchanged historical inputs, zero model-load/generation/acquisition requests,
  bounded no-match wording and no cross-scope rows. A parser-only scenario pass
  or a log line saying “ready” is insufficient.
- [ ] Freeze the new receipt and commit it with a source-bound report. Run the
  existing generated-answer compatibility cases separately; keep the 8/15
  semantic result and other platform limits unchanged.

**Done when:** all of the milestone goal passes in fresh native processes on
the recorded platform. Portable shared-library/browser compilation is a separate
check; broader headed browser, Windows/macOS/Linux and installed personal-custody
claims require their own runs. The initial native gate can use this Linux host.

#### Verification and delivery

Run focused new tests after implementation, under the existing machine-heavy
lock and the current workspace build/cache guard. Reuse approved targets, check
available capacity, and serialize builds. Commands below are **planned commands**,
not checks executed during this documentation pass; every test filter must run
a positive test count. Use each repository's pinned toolchain and lockfile.

```sh
# Eidetic M1: current shared integration tests and package regressions.
cargo test --locked -p eidetic-search --test model_free_recall
cargo test --locked -p eidetic-search
cargo check --locked -p eidetic-search --no-default-features
cargo check --locked -p eidetic-search --no-default-features --target wasm32-unknown-unknown
cargo tree --locked -p eidetic-search -e normal,build,features
# Mere: retained generated-answer adapter remains compatible.
cargo test --locked -p distillery --features inference-model --lib inference::recall::tests
# Eidetic M4: add the core filter only once matching record tests exist.
# cargo test --locked -p eidetic --lib model_free_
# Turnstone M2–M5: unified research and existing exact readers.
# Add the model_free_ filter only once matching product tests exist.
cargo test --locked -p turnstone --lib model_free_
cargo test --locked -p turnstone --lib saved_source
cargo test --locked -p turnstone --lib research
cargo test --locked -p turnstone --lib inference
```

The M5 verifier must fail on a missing phase, zero exercised source kinds, absent
restart evidence or any failed assertion. It records software checks, source
preservation, native UI/restart evidence and publication separately. Pin updates
are delivered provider-first, then Mere, then Turnstone, directly to main after
focused final checks. Refresh upstream before each integration, preserve active
worktrees and original receipts, and rerun affected checks after source changes.

Milestone completion requires M1–M5. Each phase has an independently reviewable
result, but a library pass alone does not complete the product milestone.
Optional embeddings and generation remain comparable additions measured against
this frozen lexical/structured baseline for retrieval coverage, supported
question forms and explanation quality.

### P5. Qualify adding browsing in a browser

Build on Graphshell's extension queue and browser-hosted mere. Provide selection
of the destination mere, import range, exclusions and a preview of visits and
relationship evidence. Live remembering and explicit content capture have
separate user-configurable controls. Partial import caused by source limits or
queue bounds is visible. Tab-opening context enriches visits only when observed;
historical imports cannot invent it.

**Done when:** headed Chrome and Firefox runs import a branching trail, record
live revisits and optionally keep loaded content. Cold reopen retains visits and
edges; permission refusal and disabled capture are handled, and the chosen
mere's forget operation has tested scope. Browser persistence state, quota
failure and backup/export are visible. Ordinary capture starts no crawl. Each
receipt names its source, target, feature set and supported limits.

#### Browser history into document relations, draft 2026-10-12

Mark agrees to develop existing-browser history capture into relationships for
the document graph. This is a design amendment for P1/P2/P5, with content
observations supplied separately by P3. The encounter contract and adapters
below were proposed in this draft. The first implemented subset and its limits
are recorded under the qualification below; the remaining draft stays open.

**Source refresh at Mere `28d43dd5`.** The extension's
[live listener](https://github.com/merely-made/mere/blob/main/ports/graphshell/web/extension/background.js) selects
only the latest visit for a URL and converts referring visit IDs to URLs. Its
import limits candidates to 512 pages and retains a bounded tail of visits.
[Capture normalization](https://github.com/merely-made/mere/blob/main/ports/graphshell/src/capture.rs) also drops
same-URL observations inside a time window even when their visit IDs differ.
Its transition mapping treats `generated` as tab spawning and subframe loads
as redirects, while the
[history projection](https://github.com/merely-made/mere/blob/main/crates/eidetic/eidetic-core/src/import/history.rs)
uses a different vocabulary mapping. The
[Firefox exporter](https://github.com/merely-made/mere/blob/main/scripts/firefox_history_export.py) drops
`from_visit` identity and microsecond precision. These are repair targets, not
properties of the proposed design.

**Acquire existing records and enrich live records.** Start with the existing
Chromium and Firefox extension adapters. History permission admits a selected
import range or live remembering; additional navigation observation has its
own control. Both browsers distinguish page-level history items from individual
visits carrying a referring visit ID:
[Chrome history](https://developer.chrome.com/docs/extensions/reference/api/history),
[Firefox VisitItem](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/history/VisitItem).
Read visits for each candidate address and retain source relationships before
projecting them. History import issues zero page requests.

For live intake, serialize reconciliation per address and collect all unseen
visit IDs, with an overlapping reconciliation range after suspension. A
latest-only result or timestamp watermark cannot prove delivery of every visit.
Retry identifiers deduplicate delivery; distinct visit IDs survive proximity
in time. When an adapter has no stable visit ID, retain that limitation rather
than treating a URL/time guess as proven identity.

Import completeness is a separate result. History search returns page items,
not a paginated visit stream; in particular Firefox documents filtering by
`lastVisitTime`:
[history.search](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/history/search).
A page revisited after an import's end may still have visits inside the selected
range. Discover candidates without excluding such pages, then filter individual
visit timestamps. A candidate cap, saturated time partition, queue overflow or
source retention limit must report partial or unknown coverage. A successful
batch acknowledgement does not imply a complete range. An explicit database
export can provide another adapter, beginning with the existing Firefox script;
it must retain original IDs, parent references and timestamp precision, report
its database/schema assumptions and use an authorized consistent snapshot.

**Proposed encounter contract.** Extend existing Eidetic intake/history types
instead of creating a second browser history store. Retain:

- Stable source scope: browser product, registered profile instance and history
  generation. Import runs are delivery identity, not profile identity. File and
  extension adapters need an explicit binding when they describe the same source.
- Original visit ID and original parent visit reference, including unresolved
  parents. Missing, source-reported root, outside-range and policy-withheld
  parents have distinct states; a parent URL alone cannot identify an encounter.
- Resource locator, source timestamp with original precision, raw transition
  type/qualifiers and adapter version. Normalized transition is a projection.
- Available tab/window/frame/document observations, scoped to their source
  lifecycle. Historical titles may describe the latest page item, rather than
  the page as seen during an old visit; preserve that qualification.
- Acquisition time/device and known source locality separately. Chrome's
  `isLocal` can distinguish local from synced visits; it does not identify the
  original remote device or the persona who made that visit.
- Source material/digest and conversion notices through existing
  [Intake](https://github.com/merely-made/mere/blob/main/crates/eidetic/eidetic-core/src/import/intake.rs).
  Conflicting facts for the same source identity remain inspectable and do not
  silently replace an admitted encounter.

**Relations retain their evidence.** The following names describe meanings,
not newly selected enum variants or predicate IRIs:

| Evidence | Encounter relationship | Document-graph projection |
|---|---|---|
| Source `referringVisitId` | Browser-reported predecessor/reference | Directed address relation citing both encounters; an observed link transition may refine it to followed navigation |
| Observed new-tab navigation target | Opened from a source tab/document | Branch relation only when the originating encounter can be resolved |
| Observed redirect transition/qualifiers | Redirect step or qualified redirect observation | Redirect relation; retain intermediate-address gaps rather than inventing a complete chain |
| Reload or distinct repeat visit | Another encounter with the same resource | Additional visit/count and inspectable occurrence, including same-resource transitions |
| Captured document contains a link | Link at a retained document version/anchor | Contains-link relation; it does not establish a visit or a click |
| Explicit clip from a captured version | Clip derived from source and selection | Provenance relation citing the retained version, selector and originating encounter when known |
| Temporal proximity or model output | Candidate association | Separately labelled derived suggestion; it does not establish navigation parentage |

Live tab/document enrichment uses separately admitted observations such as
[Chrome webNavigation](https://developer.chrome.com/docs/extensions/reference/api/webNavigation)
and [Firefox navigation targets](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/webNavigation/onCreatedNavigationTarget).
A new-tab event identifies source and target tabs, not automatically a history
visit ID. Preserve the observation when correlation is ambiguous. A subframe
parent document is containment context, not a referring visit. Same-document
navigation can change an address without proving a new content version.

**Admission and projection.** Eidetic retains source evidence; Pandect and the
receiving host freeze the selected mere and attribution for the batch. Admit
resolved visits through Chartulary/Stemma with a stable foreign-to-local identity
mapping, then derive access records, recall traces and document relations from
those admitted encounters. Every derived edge must expose its supporting source
handles; aggregation over a pair of documents must retain the individual visits.
URL identity does not prove captured-byte identity. Unvisited link targets may
appear as clearly unvisited references without triggering acquisition.

Stemma's current
[visit_entry](https://github.com/merely-made/mere/blob/main/crates/eidetic/chartulary/src/stemma/mutations.rs)
uses the owner's current visit as parent and moves that cursor. Import needs a
reviewed explicit-parent admission seam, not replay of a chronological list
through the live cursor. Keep unresolved/conflicting/non-tree relationship
evidence in intake; do not rewrite historical parentage or a live cursor merely
to make a branch fit. The existing
[lineage projection](https://github.com/merely-made/mere/blob/main/crates/eidetic/eidetic-core/src/browsing/lineage.rs)
is a starting point, but its URL-based TraceEvent projection must gain source
references before it can support lossless encounter inspection.

**Controls and custody.** Preview visits, supported relations, omitted facts,
destination, range and exclusions before import. Live remembering, content
capture and goal-directed crawl remain separate controls. Apply exclusions to
relationship endpoints as well as destinations; withholding a parent must not
leak its address through an edge, source envelope or notice. Acknowledge only
durable admission, and refuse a late result for a changed destination. Queue
limits and data loss are visible. Forget scope and re-import behavior are an
explicit user choice; browser deletion does not silently dictate mere deletion.

Private deployment requires custody of the entire path: extension staging,
source evidence, encounter records, graph snapshots and indexes. The current
queue uses ordinary extension storage, and capture calls the unsealed access
writer even though a
[sealed writer](https://github.com/merely-made/mere/blob/main/ports/graphshell/src/access.rs) exists. Sealing only
that payload would not qualify the other copies. A locked/unavailable destination
must visibly pause capture or use an explicitly supported bounded sealed inbox.
Keep this gate separate from synthetic unsealed graph fixtures.

**First implementation slice and done-conditions.** Repair the encounter
contract and existing extension/import adapters, then qualify admission and
graph projection with a synthetic branching history: A leads to B and C, B is
revisited twice rapidly, and one child arrives before its parent. Re-import and
cold reopen preserve the encounters and derived edges; another profile reusing
the same visit IDs stays distinct. Include an outside-range parent, excluded
parent, ambiguous tab correlation, same-resource transition, subframe/generated
transitions, failed-write retry, full queue, tied search results and a page whose
latest visit lies after the selected range. Prove zero destination fetches and
no live-cursor movement. Expose a relation's exact supporting encounters in the
graph. Renew Chromium and Firefox headed receipts separately when their runs
are authorized; other browsers require their own adapter/evidence review.
Content clipping, broader formats and sealed custody then exercise this same
admission boundary, with their own consumer gates.

#### First browser encounter qualification, 2026-10-12

Implementation `8c490e23` adds the portable source scope, parent states and
original-time fields in Eidetic, plus Chartulary's explicit-parent `admit_visit`
seam. The source owner keeps no imported live cursor. Graphshell stores the
Stemma snapshot and unresolved intake under the receiving graph's identity,
using the existing Muniment batch alongside graph/access/trace writes.
Resolved parentage produces actual document traversals and typed evidence
facets; `MereHost::browser_relations` exposes exact source handles and distinguishes
followed links, redirects, reloads and other browser-reported referrers.

Native library regression passes 67 Chartulary, 132 Eidetic and 290 Graphshell
tests, with five existing ignores and one explicit disk-receipt fixture ignored
by the ordinary sweep. That fixture passes in separate seed/reopen processes
over Redb: five source visits and three relations retain the same graph,
encounter identities and evidence, and re-import adds none. Focused cases also
cover rapid same-address visits, branching and late parents, profile ID reuse,
outside-range/withheld parents, conflict/capacity/cycle refusal, failed batch
retry, explicit forget and unchanged existing navigation cursors. Removing
parentage makes the branch test fail, followed by byte-exact source restoration.

The real extension model/background scripts pass Chromium and Firefox API
simulations for stable worker scope, rapid revisits, per-visit range filtering,
candidate saturation and visible queue loss. Their generated payload also
passes through Rust deserialization and graph admission. The Firefox SQLite
exporter preserves original parent IDs and microseconds with explicit registered
scope inputs. The standalone Graphshell browser target compiles for wasm32.
History intake calls browser history APIs and local storage, without requesting
destination pages. A native-only test dependency enables Muniment's existing
Redb backend; supplier pins and shared clip schemas are unchanged.

See [machine-readable evidence](https://github.com/merely-made/mere/blob/main/support/browser-history-intake/qualification.json)
and its adjacent raw logs and synthetic disk expectation. These qualify one
receiving writer and synthetic custody, not concurrent independent writers or
real private browsing. The portal's new awaiting-parent feedback has compilation
and API evidence, not renewed headed UI acceptance. Actual Chromium/Firefox
extension lifecycle and IndexedDB reopening, live tab/window/frame/document
correlation, source replacement/reinstallation binding, full input envelopes,
recall-trace source handles and sealed custody remain open. Native forgetting
removes encounter and relationship evidence while preserving independently
owned resource-registry identities; it is not a whole-mere identity purge.

### P6. Qualify storage, custody, retention and exchange

Make selective replication of a portable collection a forcing consumer here,
following P2/P3 rather than waiting for model work. Assemble the selected roots,
membership and meaningful edges plus the profile's required bytes, schemas and
assets through a bounded, cycle-aware traversal. Reuse bundle resolution and
hash verification, with explicit limits and missing/denied/unsupported outcomes.
Required bytes must actually be available at the receiver before it reports a
complete offline copy. Optional material and external references remain visible.
Destinations admit the collection through their existing attributed write and
retention paths; retry cannot duplicate admitted content or erase its lineage.

For shared community collections, reuse Gemot's contribution/collection authority
and Stickleback's accepted-operation replication under the
[community collection plan](https://github.com/merely-made/mere/blob/main/design_docs/moothold_docs/implementation_strategy/2026-06-12_moot_object_m1_plan.md#community-collections-and-author-offline-publishing-2026-09-04).
Private cross-mere copying does not require community publication. Collection
membership replication and delivery of the corresponding byte payloads each
need qualification. Later subscriptions must define membership/revision updates,
local edits and withdrawal rather than inheriting snapshot-copy semantics.

Reuse Muniment's existing backend contract and the host's custody/sealing owners.
Compare native and browser workloads under equivalent durability requirements;
the June OPFS-versus-Fjall numbers used different flush behavior and cannot
select a new backend by themselves. An additional OPFS adapter or packed layout
requires a demonstrated unmet workload and belongs below Eidetic. Requesting
persistent browser storage can be refused. [MDN StorageManager.persist](https://developer.mozilla.org/en-US/docs/Web/API/StorageManager/persist).
Retain the [archived-plan tails](https://github.com/merely-made/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-07-03_archived_plan_tails_plan.md)
checkpoint on whether an additional adapter must implement `Backend::transact`
before consumer adoption; browser storage availability alone does not settle it.

Define how encounters, captures, schemas, model/corpus references, tombstones and
derived indexes participate in retention. Preserve named checkpoints for deletion
and shared-reference semantics before implementation. Reuse current signature,
schema and transport admission for exchange; disclose unfamiliar payloads without
claiming to interpret them. Community recall starts from admitted source records
and local reminting. The former `EngramDirectory` proposal needs a concrete
large-corpus consumer and a fresh comparison before returning as a dependency.

**Done when:** real backend close/reopen and failure fixtures preserve atomic
admission; quota and persistence refusal have supported outcomes; collection
cannot remove material still required by an admitted retained reference under
the selected policy. A supported export/import round trip retains encounter
identity and evidence. For any peer consumer, existing admission rejects altered
or unadmitted input and preserves supported foreign fields. Backend/format
migrations include recovery and measured gates, owned jointly with their
existing migration plans. Remote erasure is not inferred from local forgetting.

**Portable-collection gate:** a selected collection containing a note, a document
clip with a required image, and one recognized structured export moves between
native and browser-hosted meres. Stop the source apps and origin services, then
restart the receiver: it still renders/inspects supported content, resolves its
parts and selectors, and shows preserved provenance and relationships. Omit an
unselected item, retain an explicitly external relation, exercise a missing
required asset and unsupported schema, and verify a retry and local derivative
keep their declared identities. A two-device receipt separately qualifies peer
delivery. Source review, generic cards or successful blob transfer alone do not
pass this gate.

**Knot/Redshank host gate:** a real shared host receives a Knot selection and
a Redshank text/voice note, then reopens with both source apps stopped. Private
source backing is retained where available and absent from selection-only
sharing. Voice-body bytes are present; missing recording bytes give an honest
playback-unavailable state. Exercise explicit excerpt export separately. Verify
selector units, unknown schema retention, two tag concepts with equal labels but
different owners, exact asserter provenance, and acceptance of an engine tag
suggestion. Queue one supported edit while its owner is absent; reconnect shows
review, re-resolves the source and refuses stale/denied requests. A current
reviewed edit has an owner-issued durable outcome, with retry/deduplication
qualified separately from message acceptance. The structured-export gate above
remains a later heterogeneous extension rather than an entry prerequisite.

## 5. Order and decisions still to make

**Priority update, 2026-10-26:** P4a is the next milestone, in M1–M5 order.
Its current-code foundation is already integrated; optional model-quality work
does not block it. The broader dependency order below remains background for
the other open phases, not an instruction to redo their completed slices.

Original dependency order: **P0 -> P1 -> P2** with the bounded visit-preservation slice. P3 can follow
the identity/admission contract; P4 consumes P1/P2 evidence and P3 captures.
P5 first qualifies metadata intake after P2, then content capture after P3.
Storage/custody review in P6 accompanies all durable changes; new backends and
federated consumers wait for their demonstrated need. Qualify a native consumer
and browser consumer separately, and record compile, automated, headed and
physical evidence separately.
The portable-collection profile starts in P0; producer support follows P3 and
P6's selected-copy gate follows P2/P3. This core portability result does not
depend on the later inference or community-search slices.

The implementation proposals still need these concrete decisions:

1. Portable encounter envelope and its mapping to native Stemma identity;
   supported old records and handling of malformed or contradictory parent facts.
2. Session/domain scope of imported and live encounters, destination selection,
   and configurable retention defaults. Engine website storage remains under
   the [native session-store plan](https://github.com/merely-made/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-06-23_native_session_store_plan.md).
3. The first engine pair, embedding-envelope consumer and Distillery inference
   host, including device injection and browser executor support.
   *2026-10-15:* the host is ruled: djinn's resident, with recall over
   retained material as its forcing consumer and first envelope consumer
   ([resident inference and recall plan](https://github.com/merely-made/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-10-15_resident_inference_and_recall_plan.md)).
   The engine pair and browser executor remain open.
4. Deletion/reference semantics, any demonstrated backend gap, accepted-schema
   discovery and the first exchange consumer. These are named checkpoints;
   the plan does not choose a universal database or automatic sharing policy.
5. The first portable-content profile, supported receiver formats/operations,
   required dependency rules and preservation of collection relationships. A
   selected snapshot and an ongoing synchronized collection have separate gates.
   Knot/Redshank now supply the first pair; exact wire fields and capability
   promises remain draft choices. CP1 to CP4 have settled their named policies.
6. Source-tag copying onto derivatives, rights/revocation and deletion/reference
   consequences, and measured backing/queue budgets. The safe draft preserves
   source tags as related facts and creates new clip assertions only through
   an attributable action; that copying policy remains a named checkpoint.

## 6. Progress

- **2026-10-26, model-free milestone planned.** Refreshed Eidetic, Mere and
  Turnstone upstream, checked integrated cross-source ancestry and current
  source owners, and specified P4a M1–M5 in this existing plan. The plan keeps
  exact evidence/admission and optional inference contracts, separates current
  head from historical availability, and defines bounded structured operations
  with an offline three-source native restart gate. Source, documentation and
  receipt inspection only; no implementation, build or new runtime qualification.

- **2026-10-26, lexical recall source home.** Promoted the tracked `eidetic-search`
  package into `crates/eidetic-search`, added it to the workspace, and wired
  Eidetic's local core plus Genet `01d3f528` and ESP `bd0468c5`; the rewritten
  Genet commit matches the donor tree recorded in
  `Code/output/graphshell-components/validation-resumed/search-supplier-equivalence.json`.
  That receipt records ESP candidate `7062f467`; it does not establish tree
  equivalence for the current ESP pin `bd0468c5`. All eight library Rust files
  are byte-identical to their Mere donor; differences are confined to example
  and test documentation. The locked, offline Rust 1.99.1 package test receipt
  records 40 passed and 3 ignored. Existing package identities and dependency
  edges were preserved, with optional ESP dependency edges added. The locked,
  offline Rust 1.99.1 `eidetic-recall` example check passes with the
  `recall-example` feature; it verifies compilation only. Provider source is
  published at `354ab71`; Mere `b232631f3` removes the donor and consumes that
  exact provider. Turnstone `309ea73` also consumes it directly and passes all
  nine inspection-pane tests after the source-owner cutover. The other receiving
  hosts consume the qualified Mere source; no compatibility Search facade remains
  in Mere. Raw checks and publication receipts are retained in
  `Code/output/graphshell-components/validation-resumed/`. The source transfer,
  provider checks and direct receiving cutovers are complete.
  These results do not qualify trained-model retrieval, semantic answer quality
  or WGPU execution, and do not close the broader P4 model, assessment and
  tag-work requirements.

- **2026-10-19, extracted ownership and native continuation.** Refreshed the
  supplier/consumer source homes and recorded the accepted producer-to-retention
  order, existing authored-note/R4 receipts, missing production exporter calls,
  receiving format limits and custody prerequisite above. Rechecked all 413
  Turnstone source hashes for the authored-note receipt, its 71 packet hashes
  and the original receiver's 110 packet hashes with no mismatches before the
  source cutover. The cutover itself does not inherit those runtime receipts.
  This is preparation and source/evidence review; no new native run, runtime
  implementation, stored format or qualification result is claimed.
- **2026-10-11.** Expanded the Eidetic development direction with Mark's visit,
  related-crate and dependency-use requirements. Reviewed current intake,
  branching, capture, session, model and storage seams. Archived the June
  deferred plan and carried its relevant tails forward. No runtime code changed;
  implementation and source-qualified consumer receipts remain pending.
- **2026-10-11, collection clarification.** Mark named selective replication
  of portable app content as a required result. Added the shared profile,
  existing bundle/manifest/collection inventory and native/browser offline-copy
  gate. Content portability is core work alongside P2/P3, with peer delivery and
  ongoing synchronization qualified separately. No implementation started.
- **2026-10-11, Knot/Redshank handoff.** Verified Mark's forcing-pair request,
  four policy answers and authorization to coordinate in the dynamics chat.
  Accepted shared profile, owner-adapter and tag design here. Recorded CP1 to CP4
  once in the family brief, drafted the adapter/profile requirements and added
  offline disclosure, tag attribution and reviewed reconnect gates. Existing
  dynamics pauses and unrelated editor work remain unchanged. No schema was
  frozen and no runtime implementation or new execution receipt was produced.
- **2026-10-11, first transfer draft.** Implemented the records, bounded selected
  envelope, Knot/Redshank producer exports and Fleece intake helpers above.
  Native Fleece tests pass (7 traversal, 6 transfer/schema cases); the enabled
  library compiles for `wasm32-unknown-unknown`. Shared-checkout owner regression
  checks pass: Knot composition 48 tests with one optional OEWN fixture ignored;
  Redshank model 33 tests. These are automated source/byte/store receipts, not
  headed host, peer-sync or physical audio acceptance. Exact committed producer
  bases pass their focused export tests and reproduce the transfer fixture
  hashes: Knot `21e1b0f6f739ef023fc71152fb1f8ce80354a3e2`; Woodshed
  `a7dcb54066d37ec53b182d7ffe75dd6952fb09df`, published through the source-identical
  documentation merge `961731ace9ae9e687bdf8d40a032f2e1fbbf3321`. Only owned
  export paths were integrated; temporary collision worktrees were removed.
- **2026-10-11, native receiver slice.** Turnstone `d2fb07c` admits selected
  packages to the receiving mere's Muniment/Fjall store, resolves retained
  addresses locally with NoFetcher, lowers a real Inker view and plays held WAV
  bodies on its existing Redshank output runtime. Receipt publication `bb09931`
  qualifies import, package-unavailable fresh-process reopening and missing
  local identity refusal in Windows, with nine reviewed native captures. The
  reopened Play link reaches Ended 500 ms through ordinary pointer routing.
  All accepted runs have zero acquisition attempts and eager peer services;
  62 sampled TCP/UDP endpoint observations are empty. The broad library gate
  passes 789 tests with ten existing ignores; six final receiver tests and one
  offline-port test cover the later focus repair and host-only read/control
  hardening. One immutable Mere source supplies 75 resolved packages after
  coherent producer/renderer/identity adoption. This is a synthetic text/WAV
  native receiver receipt, with unsealed retained storage; acoustic quality,
  browser/structured receiving, recording excerpts, tag acceptance, reviewed
  reconnect edits and P6 custody/retention remain open.
- **2026-10-12, selected onward copy.** Turnstone implementation `97c1394` and
  qualification `9412077` expose the existing Fleece selected-body exporter
  through a host-only clip link and contextual command. A first receiver with
  original package paths unavailable exports only chosen Knot/WAV clips; a
  second independent native mere receives their unchanged envelopes, retains
  the same content references and source facts, and reopens/plays after both
  sending app and all package paths disappear. The unselected note stays
  unavailable. Nine onward captures qualify the default Windows host; 794
  library tests pass with ten existing ignores, and ten final focused tests
  include changed/missing held-body refusal, budgets, excluded backing and
  unselected data, distinct tag-owner metadata, frozen Workbench selection and
  replacement policy. No schema, collection format, dependency or supplier pin
  changed. This uses the existing bounded dependency closure and NoFetcher.
  The store stays unsealed. Browser/structured receiving, recording excerpts,
  signed/accepted tags, reviewed owner edits, peer delivery and P6 remain open.
- **2026-10-12, browser-history relation design.** Refreshed the existing
  extension, imported-history, access, Stemma and lineage source seams at
  `28d43dd5`, and checked official Chromium/Firefox history and navigation APIs.
  P5 now specifies source-scoped encounter identity, retained parent references,
  raw transition evidence, visible import coverage and evidence-backed document
  relations, plus an explicit-parent admission seam that preserves live cursors.
  The forcing fixture includes rapid revisits, branches, out-of-order parents,
  redacted/outside-range endpoints, source-ID collisions and partial imports.
  This is a documentation-only design amendment; runtime, schema, custody and
  renewed headed browser qualification remain open.
- **2026-10-12, first browser encounter implementation.** Source `8c490e23`
  connects source-scoped Eidetic intake and explicit-parent Stemma admission to
  Graphshell's document graph. Native regression passes 489 tests, with six
  ordinary-sweep ignores; the new disk fixture separately passes seed/reopen
  processes and idempotent re-import. Production JS/Rust wire and simulated
  Chromium/Firefox adapters pass, the browser target compiles, and a restored
  parent-removal control demonstrates branch-test sensitivity. Qualified scope
  and open custody, multi-writer and headed-browser gates are recorded under P5.

- **2026-10-19, custody and first producer slice verified locally.** Mere's
  scoped custody adapter, explicit keeper namespace configuration, stable
  profile/lock admission and historical-epoch loading are implemented.
  Pandect's optional purpose scope preserves the canonical content hash and
  existing unscoped associated data. Native checks pass: endpoint 43, wallet
  sealer 6, and actual custody IPC/existing route 3. The receiver's 54 focused
  retained-name regressions and two captured producer-action cases pass;
  Redshank's owner export 2, playback 32 and surfaces 103 pass. Process/device
  helpers remain separately scoped from ordinary sweeps.
  The separate wallet-backed process receipt exports a real owner voice body,
  removes the source blob and transfer packages, restarts both custodian and
  receiver, and reopens selected Knot and voice content with preserved UTF-8
  and millisecond selectors. Source hashes, artifact hashes and scope live in
  `turnstone/docs/receipts/2026-10-19_retained_custody/qualification.json`.
  Turnstone work remains isolated in `Code/worktrees/turnstone-eidetic-retention`;
  Redshank work is in `Code/worktrees/woodshed-eidetic-retention`, preserving
  shared concurrent edits and divergent Woodshed main. The receiver used
  explicit local supplier overrides, without changing canonical product pins.
  Publication/pin refresh, renewed headed interactions, installed personal
  custody/migration and physical audio remain open. This checkpoint does not
  close P6, typed/browser receiving, signed tags or reviewed reconnect edits.

- **2026-10-19, suppliers published and bounded headed continuation accepted.**
  Mere custody `e122a071` is published and consumed through `48fd338`; Knot's
  Eidetic-family alignment is `6e721d3`; Woodshed's selected voice exporter and
  missing-recording menu-layout repair are consumed through `62dde4c`.
  Turnstone's refreshed canonical Git lock uses Eidetic `543f5ff` and Genet
  `83114ac2`, without local supplier overrides. The implementation and consumer
  pins remain uncommitted in `Code/worktrees/turnstone-eidetic-headed`; this
  checkpoint records completed supplier publication and local acceptance.
  Standalone Redshank exports its selected shareable owner WAV through visible
  controls. The embedded saved-document keyboard path selects `café` at UTF-8
  bytes 16–21; full source backing remains private and onward transfer contains
  only the selected text. A second isolated receiving store reads that selection
  and plays the selected voice after source applications stop. With source
  files and all transfer packages removed, a fresh receiving process plays to
  `Ended 100 ms` and exports both clips again with unchanged selected package
  data and zero acquisition attempts. A custodian restart also succeeds, and
  retained-store checks find no clear full-source/WAV bytes or WAV cache files.
  The final shipping build, 15 focused clip checks and four selector/action
  cases pass. The earlier 56-case integration sweep and its different lock are
  preserved separately. Five native consumer phases, twelve reviewed captures,
  exact source/binary hashes and limits are in
  `turnstone/docs/receipts/2026-10-19_retained_custody/headed/qualification.json`
  within that local worktree; the standalone owner receipt lives in
  `woodshed/docs/receipts/2026-10-19_selected_note_export/qualification.json`.
  Embedded document drag selection forwarding remains open; this acceptance
  uses the normal keyboard input path. The native destination dialog,
  installed personal custody/migration, microphone/acoustic quality, renewed
  headed embedded Redshank text-note retention and broader browser/structured
  receiving keep their own gates. No whole-P6 or heterogeneous-collection
  completion is claimed, and frozen earlier packets are unchanged.

- **2026-10-19, consumer publication follow-up.** Turnstone's qualified custody
  integration, tested supplier pins and frozen receipts are now published on
  main in [01658c739dd1f566b0317578ff5c94efa74720fe](https://github.com/merely-made/turnstone/commit/01658c739dd1f566b0317578ff5c94efa74720fe).
  The earlier local-state checkpoint records the state at qualification time.
  Publication preserves its source hashes and qualification limits.


- **2026-10-20, post-extraction stack checkpoint.** Reviewed published supplier
  and consumer manifests/locks, normal versus dev edges, backend transaction
  capabilities, current recall persistence and independent CI. The stack review
  above records ownership, bounded evidence and open contracts. Fresh locked,
  offline metadata verifies the normal/build closure excludes Mere's application
  source; local README links and `git diff --check` pass. Refreshed the
  repository and core READMEs: lexical recall is BM25 and its ESP edge is
  example/dev-only. No runtime formats, APIs, pins or implementation changed.
  The October 20 Turnstone Knot and Redshank receipts extend the native host
  evidence without closing all P4/P5/P6 requirements. This is source/receipt
  assessment and documentation work, not a renewed build or host qualification.


- **2026-10-20, embedding envelope implementation started.** The retained
  contract lives in `models::embedding`: exact provider/transform identity,
  artifact digests and settings, numerical shape/metric and a versioned chunker
  over unique exact source revisions. ESP supplies the provider-bound guarded
  index path; the resident consumes it without changing the generic numerical
  index API. Qualification requires a real provider over retained source text,
  separate-process cold reopen, same-dimension foreign-space refusal, stale
  source/chunker refusal and a fixed held-out lexical/fused comparison. Runtime
  source authority and private custody stay with the existing owners. This
  checkpoint is in progress; no acceptance claim is made yet.

  The contract's focused checks pass (two cases covering matching serialization,
  thirteen same-dimension identity/settings changes, unknown versions, duplicate
  source identities and missing revisions). This publishes the retained schema
  for downstream ESP integration; resident qualification remains in progress.

- **2026-10-20, embedding envelope consumer qualified.** The published retained
  contract `34105d63` and ESP guarded implementation `6d371901` compose in Mere
  `fb56a2ae1`. The resident I10 gate passes matching separate-process cold reopen,
  exact retained-source passage identity, same-dimension foreign-space refusal,
  source/chunker/legacy refusal, a bounded vector index and the fixed
  lexical/vector/fused comparison. The canonical receipt is
  `mere/support/receipts/resident-embedding-20261020/qualification.json`.
  The real MiniLM retriever is paired with a tiny exact-session decoder fixture;
  this receipt measures retrieval and native IPC, without a new trained-answer,
  headed UI, browser, assessment/tag or second-device verdict.
