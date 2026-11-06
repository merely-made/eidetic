# Memory gates plan: Eidetic's open phases after P4a

**Status:** 2026-11-01: plan, a draft awaiting review. Cut from
`2026-10-11_eidetic_development_plan.md`, archived whole in
`../../archive_docs/2026-11-01/` (D1). Landed there: P4a, the model-free
retained research milestone M1-M5, complete for the recorded Linux source
cohort (Turnstone `554f323f`, Mere `845fe1c4`, Genet `b7c0bef`); the embedding
envelope (2026-10-20, qualified through Mere's resident I10 gate); the inference
surface (resident I5); the first browser encounter slice (`8c490e23`,
2026-10-12); the first portable-clip transfer draft and its native receiver
(2026-10-11, 2026-10-12); the native Knot and Redshank custody, retention and
onward-export receipts (2026-10-19, 2026-10-20); and lexical recall's source
home in `crates/eidetic-search`. Open: the capability ledger (P0), the
encounter, admission and capture gates the 2026-10-12 qualification left open
(P1, P2, P3), P4's remainder, which is next (D2), headed browser qualification
(P5) and storage, custody, retention and exchange (P6).

The keyword is `memory_gates`. The archived plan's keyword, `eidetic_development`,
named the founding program, which has landed its model-free baseline. What
remains is one build (P4's goal/query consumer and tag suggestions) and a set
of named qualification gates over built code, each saying what a receipt must
show before a claim widens. Phase numbers keep the archived plan's, because
the Esp, Mere, Turnstone and Woodshed plans cite its phases by name.

**For:** finishing Eidetic's retained-content and memory program: encounters
preserved through intake, history and captured content admitted to the chosen
mere, recall and model work over named evidence, browsing in a browser, and
storage, custody and exchange of portable collections, each with a receipt
that names its source, host and limits.

Not here:
- The M5 follow-ups, improved clipped labels and automatic restored-pane
  recovery: Turnstone work, in `turnstone/design_docs/`. The M5 receipt bounds
  its own claim without them.
- Generated-answer quality (the 8/15 semantic score) and the optional-inference
  path: Distillery and Turnstone's `cross_source_answers_20261022` packet, and
  `mere/design_docs/mere_docs/implementation_strategy/2026-10-28_resident_inference_plan.md`
  and `2026-10-28_distillery_models_plan.md`.
- Dependency version moves and lockfile migrations:
  `mere/design_docs/mere_docs/implementation_strategy/2026-10-28_workspace_hygiene_plan.md`.
  This plan supplies use cases and gates for Fjall storage, JSON Schema and
  tokenizers; it starts no competing migration.
- Community collection publication and subscriptions:
  `gemot/design_docs/moothold_docs/implementation_strategy/2026-11-01_moot_publication_plan.md`.
- The source-size check and prek hooks: `2026-10-30_line_ceiling_plan.md`.

Receipts from this plan's runs go to
`archive_docs/receipts/2026-11-01_memory_gates_plan_receipts.md`, never into this
plan (DOC_POLICY §8). The archived plan's receipts stay in
`archive_docs/receipts/2026-10-11_eidetic_development_plan_receipts.md`; the M1-M5
run outputs they cite are at `Code/testing/eidetic/` (D3). Evidence that matters
is a test in the repository or a line in Findings.

## Constraints

Carried from the archived plan; none is a new ruling.

- **Host-owned authority; existing formats.** Eidetic keeps retained facts
  interpretable independently of an open browser or loaded model. The ownership
  table below does not authorize changing domain semantics or replacing stored
  formats silently; new records follow the workspace data-format rulings
  (`repos/mere/design_docs/2026-10-06_data_formats_brief.md`), and existing
  signed, hashed or persisted bytes need explicit compatibility work.
- **Extend before adding.** Extend existing modules before proposing further
  crates, a database, a query-language runtime or an execution supervisor.
- **Stay inside the cohort.** A receipt covers its recorded source cohort and host.
  A pin difference alone neither invalidates a frozen native receipt nor
  justifies a blanket relock. Compile, automated, headed and physical evidence
  are recorded separately; native and browser consumers are qualified separately.
- **No acquisition by intake.** Importing history, gathering loaded links and
  retaining a clip fetch no destination page. A crawl is an explicit,
  bounded, cancellable action with its limits recorded.
- **Dependency use is part of every phase.** Each phase updates the P0 ledger
  rows it touches with the API used, source and version, feature set, target,
  consumer and validation, and a disposition: use as supplied, adapt, repair
  our use, upstream fix, or replace with evidence. Do not claim dependency
  counts or browser suitability from manifest inspection.
- **Test sealers are not custody.** The portable test sealer is not
  cryptographic product qualification.

| Owner | Responsibility | Consumer boundary |
|---|---|---|
| Muniment | Byte backends, immutable blobs, mutable slots, journals, atomic storage operations | The host chooses a backend and custody policy |
| Chartulary / Stemma | Resource graph, attributed edits, individual encounters, owner-scoped branches | External observations are admitted through explicit mappings; arena handles are not portable visit identities |
| Eidetic | Portable intake, typed retained content, schemas, source evidence, browsing corpus, model/corpus/evaluation artifacts | Keeps retained facts interpretable independently of an open browser or loaded model |
| Pandect | Session manifests and lifecycle, graph/reference adapters, settings, restoration, graph-aware retention | Session attachment and saved view state stay distinct from durable facts |
| Inker | Engine routing and the document/session/capture contracts engines implement | Reports what the selected engine can actually expose |
| Fleece | Extraction, clipping, observed content links, bounded traversal over supplied sources | Source acquisition and session admission stay explicit adapters |
| ESP / eidetic-search | Model execution, embeddings, numerical retrieval, lexical recall, ranking fusion | Results derive from named source versions and settings |
| Distillery | Model selection, supervised jobs, execution status, reusable work surfaces | Uses ESP for execution and Eidetic for artifacts; the owning application admits proposed edits |

## Phases

### P4. Recall and model work over named evidence: the remainder (next, D2)

*Landed (archived plan P4): the embedding envelope, 2026-10-20.
`EmbeddingEnvelopeV1::ensure_compatible`
(`crates/eidetic-core/src/models/embedding.rs:126`, commit `937d777`) with ESP's
provider-bound `compatibility_space()`
(`esp/crates/esp/src/embed/provider.rs:75,117,156`), qualified through Mere's
resident I10 gate (`mere/support/receipts/resident-embedding-20261020/qualification.json`):
cold reopen accepts a matching space, refuses a same-dimension foreign space and
changed source, chunker or legacy records, and measures lexical, vector and
fused recall on a frozen corpus. Also landed: the inference surface (resident
I5: `models::inference::InferenceRunRecord`, `models::recall::InferencePassage`,
streaming, pause, resume, cancel and reopen of a past answer with the origin
offline), and the bounded answer contract (`models::answer::AnswerSupport`,
focused coverage; its model qualification is tracked in Turnstone's packet).*

The remainder is a goal/query consumer and tag work, with the archived plan's
constraints:

1. **Goal/query consumer.** Compose one real consumer that takes a goal or
   query, directs Fleece traversal over supplied sources (bounded by node, depth,
   fan-out, request and byte limits, with cancellation, omissions and source
   receipts), and assesses retrieved material through ESP. Results cite exact
   retained source versions and explain the traversal choices. Graph changes
   are proposals the owning application admits. Alembic's workshop
   (`mere/crates/distillery/alembic/src/workshop.rs`) already owns shared
   selected-source proposals, review and acceptance; reuse it rather than
   building a second review surface. Alembic selectors are UTF-8 byte offsets
   and Eidetic passage selectors are Unicode scalar offsets, so any handoff
   converts against the exact retained bytes with boundary checks.
2. **Tag suggestions (CP3).** Generated tag suggestions are queryable with their
   producer, run, input revision, settings and review state, outside asserted
   personal `taggedWith`. Accepting one creates a personal assertion citing the
   suggestion; rejecting or regenerating cannot rewrite existing personal tags.
   Reuse identity-owned `TagConcept` and attributed resource statements. A
   suggestion is queryable at its declared keeping tier without becoming a
   personal tag or requiring an explicit clip promotion.
3. **Lifecycles.** Runtime observations, retained clips and notes, and
   rebuildable indexes keep separate lifecycles; evidence and review state
   survive reopening.

Slices, in order (2026-11-05):

- **S1, model-free goal assessment (Eidetic).** `eidetic_search::goal`:
  candidate selection by goal-term overlap of a known label and source
  assessment by goal-term coverage, each choice kept in a `GoalRecord` that
  cites retained version, text hash, matched and missing terms, scalar
  selectors and the selection rule. Done when it is the ordinary no-model path
  for S2. Landed (see Progress).
- **S2, the Fleece adapter (Mere).** A retained-source provider and the S1
  selector and assessor plugged into `fleece::traversal::run_traversal`, with
  the request and byte limits the plan names and Fleece lacks (it bounds nodes,
  depth, fan-out and pending only). The report and `GoalRecord` travel
  together. Done when a traversal over supplied retained sources makes zero
  fetches, reports omissions and cites versions.
- **S3, the model path (ESP).** An ESP assessor that reranks or replaces S1's
  verdicts and records its model, settings and envelope beside the S1 record;
  the S1 path still runs when no model is present.
- **S4, tag suggestions (Eidetic records, then the owning graph).** Portable
  suggestion, review and assertion-draft records in Eidetic; the owning
  application admits the draft as a personal assertion through its attributed
  funnel.
- **S5, the host.** A person-given goal in a native host, results and
  proposals through Alembic's workshop with byte-to-scalar selector
  conversion, and the headed receipt the done-clause asks for.
- **S6, native retained text (planned, D16, D17).** Mere's resident opens no
  retained page-text store in production (Findings, 2026-11-05); Turnstone's
  `trail_memory` already captures natively and qualified M1-M5. Promote its
  storage-agnostic core into Eidetic over a host-supplied `Backend`: browsing
  capture, the sealed capture writer, capture-source documents, the page
  corpus and recall minting. Turnstone keeps its armillary actor, M5 receipts
  and inspection. Natively, Pandect's reservoir supplies the store (store
  lifetimes stay with the store, distillery-extraction D19); Graphshell gets a
  native capture writer over the promoted core and a read-only, bounded
  resident-door request (`chirograph`) for the goal surface's page snapshot
  under the viewer's per-app graph scope. Done when Turnstone runs on the
  promoted core with its M5 checks intact, and graphshell-mere runs the goal
  surface over pages it captured natively, after a cold reopen, with sealed
  custody or a visible pause, and a headed receipt.

**Done when:** a real lexical or model provider produces query results over
retained visits and captured content, cites source versions and explains
traversal choices, on a goal given by a person, with the ordinary path usable
when no model is present; suggestions are queryable with their run and input
evidence, accepting one creates a personal assertion, and reject or regenerate
leaves personal tags unchanged, with CP3 evidence and review state surviving
restart; the headed receipt covers at least one native host and names its
source cohort. Review the evidence already recorded for the archived clauses
"a reusable Distillery surface streams a real run, cancels it and reopens its
record" (resident I5) and "a meaningful recall and latency comparison" (I10)
and close them here or restate them. A stub-provider pass alone does not qualify.

### P0. Owners and dependency capabilities

Inventory the read and write paths of Graphshell capture, Turnstone navigation,
portable import, engine capture and Distillery model work. Classify repeated
types by meaning before consolidating them; retain foreign transition tags
through deliberate mappings instead of assuming every enum is interchangeable.
Populate the capability ledger from locked dependency source and actual
consumers, covering the families the archived plan names: petgraph, slotmap and
Chartulary/Stemma; Muniment over redb, Fjall and IndexedDB; serde and codecs,
BLAKE3, UUID and URL canonicalization; Eidetic Bundle, manifests and schemas;
Knot composition, Redshank timed targets and graph tag statements; jsonschema;
Genet DOM, session and text contracts with Fleece; Burn, CubeCL/wgpu, tokenizers
and safetensors through ESP; and the Armillary, mesh, Fetch and Iroh adapters.
Start with the rows P4 touches, then complete the rest.

Feature review starts with Eidetic's defaults (`crates/eidetic-core/Cargo.toml`)
and ESP's optional persistence and model-session edge
(`https://github.com/merely-made/esp/blob/main/crates/esp/Cargo.toml`): which
consumers need signing and full JSON Schema validation. Separate normal, build
and dev cones, and test intended feature combinations without accidental
workspace feature unification.

**Done when:** every planned write and derived projection has a named owner;
the ledger identifies existing APIs, unused useful capabilities, duplicated or
bypassed behavior, feature costs and target limitations. Each proposed shuffle
has a forcing consumer and gate. Missing source or runtime evidence is recorded
as pending, not inferred. The portable-collection profile names its first
producers and receiving hosts, minimum supported formats and dependency and
disclosure rules; schema validation alone is not counted as a renderer or
domain interpreter.

### P1. Preserve encounters through intake

*Landed 2026-10-12 (`8c490e23`): portable source scope, parent states and
original-time fields in Eidetic; Chartulary's explicit-parent `admit_visit`;
Graphshell stores the Stemma snapshot and unresolved intake under the receiving
graph's identity. Qualified: a synthetic branching history, rapid revisits,
a late parent, profile ID reuse, outside-range and withheld parents, failed-batch
retry, idempotent re-import, cold reopen over Redb in separate processes, zero
destination fetches and unmoved live cursors. This qualifies one receiving writer
and synthetic custody.*

Open from that qualification:

- **Full input envelopes.** Retain, per visit, the source scope (browser product,
  registered profile instance, history generation), original visit ID and parent
  reference with its state (missing, source-reported root, outside range,
  policy-withheld), source timestamp at original precision, raw transition and
  qualifiers, adapter version, and conversion notices; conflicting facts for one
  source identity stay inspectable and never silently replace an admitted
  encounter. Capture-device context stays distinct from a source visit's known
  or unknown original device.
- **Live tab, window, frame and document correlation**, as separately admitted
  observations; ambiguous correlation is preserved, a subframe parent is
  containment context and not a referring visit.
- **Source replacement and reinstallation binding**: a re-installed extension or
  replaced profile binds to its prior scope by an explicit mapping, never by an
  import-run ID.
- **Concurrent independent writers** and real private browsing, which the first
  slice does not qualify.

**Done when:** fixtures for the open items above preserve a full envelope
for each of repeated visits, two branches from one parent, two profiles reusing
visit numbers, a parent arriving after its child and a parent outside the import
range; ambiguous tab correlation is retained, not guessed; a reinstalled source
rebinds by explicit mapping; two writers admit without losing or duplicating an
encounter; re-import and restart keep the same admitted identities without moving
a live cursor; existing stored records stay readable or take an explicit
supported migration; missing facts are not fabricated.

### P2. Admit imported and live history into the chosen mere

Use Eidetic intake to retain source material, digest, scope and conversion
notices. Pandect and the application session decide the destination mere and
record accepted graph and history changes through existing attributed funnels.
Durable facts, session attachment, navigation cursor and saved projection
preferences stay separate; counts and recent-page lists derive from admitted
encounters.

Open from the 2026-10-12 qualification:

- **Recall-trace source handles.** `history_event` still lowers an imported visit
  to a URL-based `TraceEvent` with a millisecond timestamp and no source handle
  (`crates/eidetic-core/src/import/history.rs:55-81`). The projection must cite
  the supporting encounters, and a relation over a pair of documents must keep
  the individual visits.
- **Sealed custody of the whole path.** The extension queue uses ordinary
  extension storage, and capture calls the unsealed access writer although a
  sealed writer exists (`mere/ports/graphshell/src/access.rs`). Sealing one
  payload does not qualify the other copies: staging, source evidence, encounter
  records, graph snapshots and indexes. A locked or unavailable destination
  visibly pauses capture or uses an explicitly supported bounded sealed inbox.

**Done when:** imported and live encounters join one queryable history in the
selected mere and reopen through Pandect. A second session sees the same durable
facts with its own view and cursor. Retry after failure is idempotent,
acknowledgements follow durable admission, and a destination switch cannot
attach a late result to another mere. Raw intake and notices stay inspectable
under the user's retention settings. Every derived edge and trace exposes its
supporting source handles. A private deployment run proves the entire path is
sealed or visibly paused, separate from synthetic unsealed fixtures.

### P3. Capture engine content and relationship evidence

Connect Inker's actual document, clip and capture capabilities to Fleece's
source and version observations. Reuse the existing annotation and selector
contracts; keep raw material distinct from normalized extraction; correlate
capture with its originating session and document version (see
`turnstone/design_docs/2026-08-28_page_capture_plan.md`). Retain separate
evidence for followed navigation, a link contained in a captured document,
clipping provenance and a model-proposed relation. An available link does not
establish the person noticed it; an unclicked link is not a negative preference.
An opaque engine reports its limitation.

Carried gate: **HTTP pooled-cancellation race.** The bounded crawler pool turns
canceled-request retries off (`mere/crates/system/fetch/src/crawler.rs:203`), so
there is no hidden pooled-request retry; the existing streaming checks do not
establish a deterministic pooled-cancellation race. Qualify that race with a
controlled negative case when the adapter can exercise pool cancellation, kept
distinct from general traversal tests.

Also open: the Knot and Redshank producer gates (CP1, CP2) and the export
preview; Fleece's `check_record` still refuses schema-bearing clip parts
(`mere/crates/fleece/src/portable_clip.rs:88`), so typed parts need an
explicitly qualified path before a structured export is carried.

**Done when:** a loaded document and a non-web or engine-generated source both
yield retained content and inspectable relationships through the same Fleece
seam. Reopening resolves the captured version and selectors; stale completion is
refused. Gathering loaded links produces zero destination fetches. Explicit
crawling respects configurable node, depth, fan-out, request and byte limits,
cancellation, omissions and source receipts. The HTTP adapter has a separately
recorded controlled cancellation-race gate. The selected runtime exporters
supply the collection profile's required material and retain exact source and
capture identity; a clip's export preview distinguishes selected content,
carried source evidence and external references; Knot and Redshank producer
gates apply CP1 and CP2, keep an unavailable recording distinct from a complete
portable note, and keep private backing outside the selection envelope.

### P5. Qualify adding browsing in a browser

Build on Graphshell's extension queue and browser-hosted mere. Provide the
destination mere, import range, exclusions and a preview of visits and
relationship evidence. Live remembering and explicit content capture have
separate, user-configurable controls. Partial import caused by source limits or
queue bounds is visible. Tab-opening context enriches visits only when observed;
historical imports cannot invent it. Candidate caps, saturated time partitions,
queue overflow and source retention limits report partial or unknown coverage;
a successful batch acknowledgement does not imply a complete range.

The 2026-10-12 simulations of the Chromium and Firefox APIs, the Rust wire
admission and the wasm32 compile do not stand in for the headed runs below.

**Done when:** headed Chrome and Firefox runs import a branching trail, record
live revisits and optionally keep loaded content. The extension lifecycle
(install, worker suspension, restart, reinstall) keeps its stable scope, and
the IndexedDB-backed mere reopens cold retaining visits and edges. Permission
refusal and disabled capture are handled, and the chosen mere's forget operation
has tested scope (browser deletion does not silently dictate mere deletion, and
remote erasure is not inferred). Browser persistence state, quota failure and
backup or export are visible. Ordinary capture starts no crawl. Each receipt
names its source, target, feature set and supported limits; other browsers need
their own adapter and evidence review.

### P6. Qualify storage, custody, retention and exchange

Make selective replication of a portable collection a forcing consumer,
following P2 and P3.

**Portable-collection gate:** a selected collection containing a note, a
document clip with a required image, and one recognized structured export moves
between native and browser-hosted meres. With the source apps and origin services
stopped and the receiver restarted, it still renders and inspects supported
content, resolves its parts and selectors, and shows preserved provenance and
relationships. An unselected item is omitted, an explicitly external relation is
retained, a missing required asset and an unsupported schema are exercised, and
a retry and local derivative keep their declared identities. Required bytes must
actually be present at the receiver before it reports a complete offline copy.
Assembly uses bounded, cycle-aware traversal and existing bundle resolution and
hash verification; `verify_required_members` checks direct members only
(`crates/eidetic-core/src/bundle.rs:144-174`), so the recursive dependency walk
is new work. A two-device receipt qualifies peer delivery separately; source
review, generic cards or blob transfer alone do not pass.

**Knot/Redshank host gate:** a real shared host receives a Knot selection and a
Redshank text or voice note and reopens with both source apps stopped. Private
backing is retained where available and absent from selection-only sharing.
Voice-body bytes are present; missing recording bytes give an honest
playback-unavailable state. Explicit excerpt export is exercised separately.
Verify selector units, unknown-schema retention, two tag concepts with equal
labels but different owners, exact asserter provenance, and acceptance of an
engine tag suggestion. Queue one supported edit while its owner is absent;
reconnect shows review, re-resolves the source and refuses stale or denied
requests. A current reviewed edit has an owner-issued durable outcome, with
retry and deduplication qualified apart from message acceptance.

**`Backend::transact` checkpoint.** Muniment's Redb, directory and IndexedDB
backends implement `transact`; Fjall inherits the default `NotTransactional`
refusal and ZIP refuses explicitly (see Findings). Decide, before consumer
adoption of any additional adapter, whether adoption requires `transact` or the
default is acceptable for that backend. Browser storage availability alone does
not settle it. The archived-plan-tails checkpoint is at
`mere/design_docs/archive_docs/2026-10-28/2026-07-03_archived_plan_tails_plan.md`.

**Deletion and reference semantics.** Define how encounters, captures, schemas,
model and corpus references, tombstones and derived indexes participate in
retention. Preserve named checkpoints for deletion and shared-reference
semantics before implementation. Reuse current signature, schema and transport
admission for exchange; disclose unfamiliar payloads without claiming to
interpret them. Community recall starts from admitted source records and local
reminting. The former `EngramDirectory` proposal needs a concrete large-corpus
consumer and a fresh comparison before returning as a dependency.

Storage comparisons of native and browser backends use equivalent durability
requirements; the June OPFS-versus-Fjall numbers used different flush behavior
and cannot select a backend. An additional OPFS adapter or packed layout needs a
demonstrated unmet workload and belongs below Eidetic. Requesting persistent
browser storage can be refused.

**Done when:** real backend close/reopen and failure fixtures preserve atomic
admission; quota and persistence refusal have supported outcomes; collection
cannot remove material still required by an admitted retained reference under
the selected policy; a supported export and import round trip retains encounter
identity and evidence; for any peer consumer, existing admission rejects altered
or unadmitted input and preserves supported foreign fields; backend and format
migrations include recovery and measured gates, owned jointly with their
existing migration plans; remote erasure is not inferred from local forgetting;
and the three gates above and the `transact` decision are recorded.

## Order

P4's remainder is the next milestone (D2). Populate the P0 ledger rows it
touches first. Then the original dependency order holds: P1, then P2 with the
bounded visit-preservation slice, with P3 able to follow the identity and
admission contract; P5 first qualifies metadata intake after P2, then content
capture after P3. P6's storage and custody review accompanies every durable
change; new backends and federated consumers wait for a demonstrated need; the
selected-copy gate follows P2 and P3. This core portability result does not
depend on the inference or community-search slices.

## Open questions

The archived plan's six decisions still to make, with where each stands. None is
decided here.

1. **Portable encounter envelope and its mapping to native Stemma identity**;
   supported old records and handling of malformed or contradictory parent
   facts. The source scope, parent states and `admit_visit` seam exist; the
   full envelope, old-record migration and contradictory-parent handling are
   P1's open items.
2. **Session and domain scope of imported and live encounters**, destination
   selection and configurable retention defaults. Engine website storage stays
   under the native session-store plan
   (`mere/design_docs/archive_docs/2026-10-28/2026-06-23_native_session_store_plan.md`,
   archived with no live cut).
3. **The first engine pair, the embedding-envelope consumer and the Distillery
   inference host**, including device injection and browser executor support.
   The host is ruled (2026-10-15): djinn's resident, with recall over retained
   material as its forcing consumer
   (`mere/design_docs/archive_docs/2026-10-28/2026-10-15_resident_inference_and_recall_plan.md`;
   live cut `2026-10-28_resident_inference_plan.md`), and the envelope consumer
   has qualified (I10). The engine pair and the browser executor remain open.
4. **Deletion and reference semantics, any demonstrated backend gap,
   accepted-schema discovery and the first exchange consumer.** Named
   checkpoints (P6); the plan chooses no universal database or automatic
   sharing policy.
5. **The first portable-content profile**, supported receiver formats and
   operations, required dependency rules and preservation of collection
   relationships. A selected snapshot and an ongoing synchronized collection
   have separate gates. Knot and Redshank supply the first pair; exact wire
   fields and capability promises remain draft choices; CP1 to CP4 have settled
   their named policies (`mere/design_docs/2026-08-12_family_composition_thesis_brief.md`,
   "Clip policy rulings").
6. **Source-tag copying onto derivatives**, rights and revocation, deletion and
   reference consequences, and measured backing and queue budgets. The safe
   draft preserves source tags as related facts and creates new clip
   assertions only through an attributable action.

## Decisions

- **D1, cut the plan, draft first (2026-11-01):** Mark: "Cut it, draft first".
  The 1,956-line `2026-10-11_eidetic_development_plan.md` is archived whole at
  `design_docs/archive_docs/2026-11-01/` with a Superseded header and its stale
  statements corrected, and this plan carries every open item. The cut is staged
  for review, not committed.

  Links: area docs · plans memory_gates · summary The development plan is cut into this plan and archived whole; the cut is a draft for review.

- **D2, P4's remainder is next (2026-11-01):** Mark: "P4 remainder". After the
  completed P4a (M1-M5), the next milestone is P4's remainder: the goal/query
  consumer over Fleece and ESP, plus tag suggestions. This supersedes the
  archived plan's 2026-10-26 priority line, "P4a is the next milestone".

  Links: area recall · draws on eidetic_development D1 · plans P4 · summary The next milestone is P4's remainder: the goal/query consumer over Fleece and ESP, and tag suggestions.

- **D3, run outputs leave the repository (2026-11-01):** Mark: "Move out of the repo".
  The M1-M5 run outputs tracked under `design_docs/archive_docs/2026-10-29/` and
  `2026-10-30/` (2,773 files, 192,628,541 bytes) are copied to
  `Code/testing/eidetic/` with their relative layout and removed from the
  tree. The receipts file stays in the repository, with its links into those
  folders turned into path cites (`Code/testing/eidetic/...`) and its recorded
  hashes and index entries unchanged.

  Links: area docs · plans memory_gates · summary The M1-M5 run outputs live under Code/testing/eidetic/; the repository keeps the receipts file.

- **D4, the M5 harness follows the outputs (2026-11-01):** Mark: "Move it too".
  The frozen M5 harness bundle (48 files of Python, shell, fixtures and its
  source manifest), which the archived plan had kept in the repository, moves to
  `Code/testing/eidetic/2026-10-30/model-free-m5/harness/`; the copy was compared byte for byte (48 identical) before removal.

  Links: area docs · draws on memory_gates D3 · plans memory_gates · summary The frozen M5 harness lives with the run outputs under Code/testing/eidetic/.

- **D5, P4 continues with S2 and the tag draft's admission (2026-11-05):**
  Mark: "S2 plus tag draft admission". After S1 and S4's Eidetic half, build
  the Fleece adapter (S2) and admit `PersonalTagAssertionDraftV1` as a personal
  assertion in Mere's graph (S4's Mere half).

  Links: area recall · draws on memory_gates D2 · plans P4 · summary P4 continues with the Fleece adapter and admission of accepted tag suggestions into the owning graph.

- **D6, browser followers queue edits (2026-11-05):** Mark: "Followers queue
  edits". A Graphshell follower holds `IndexedDbBackend::read_only()` and hands
  its edits to the lock-holding writer instead of writing; the writer admits
  them. Answers the posture brief's decision 2.

  Links: area storage · plans P1 P5 · summary Browser followers read through the read-only handle and queue edits for the writer to admit.

- **D7, the writer lock stays in Graphshell (2026-11-05):** Mark: "Graphshell
  only (as now)". Muniment supplies the read-only handle and no lock helper.
  Answers the posture brief's decision 1.

  Links: area storage · plans P5 · summary The browser writer lock and its page protocol stay host-side in Graphshell.

- **D8, migrate native redb to file format v3 (2026-11-05):** Mark: "Migrate
  native redb". Native databases move to redb's v3 format through redb 2.6's
  `Database::upgrade()`, independent of OPFS: first a Muniment release that
  upgrades on open and creates v3, then, once consumers have opened their
  files with it, the crate bump past 2.x. Answers the posture brief's
  decision 7; OPFS adoption stays behind its gate.

  Links: area storage · plans P6 · summary Native redb databases migrate to file format v3 via upgrade-on-open before the crate moves past 2.x.

- **D9, the Mere half rides the p2mere 0.9 repin (2026-11-05):** Mark: "Ride
  the 0.9 repin". Eidetic main reached p2mere 0.9 (`4aaa64d`) while the family
  still builds on 0.8, so no Eidetic rev with this round's APIs builds in the
  family yet. The Mere work waits as `agent/*` branches and merges once the
  release-log lane's 0.9 repin lands; no separate Eidetic repin runs.

  Links: area dependencies · draws on memory_gates D5 D6 · plans P4 P5 · summary This round's Mere work merges after the family's p2mere 0.9 repin instead of a separate Eidetic repin.

- **D10, S5's host is Graphshell native (2026-11-05):** Mark: "Graphshell
  native". The goal surface and its headed receipt are built in Mere's native
  Graphshell viewer over its retained captures, which already builds on this
  round's Eidetic; Turnstone and the resident host are not used for S5.

  Links: area recall · draws on memory_gates D5 · plans P4 · summary P4's goal surface and headed receipt are built in Graphshell's native viewer.

- **D11, proposals wait for Alembic's move (2026-11-05):** Mark: "Wait for the
  move". S5 shows cited results and traversal explanations; routing proposals
  through Alembic's workshop follows its move from Mere to Distillery.

  Links: area recall · plans P4 · summary Goal-run proposals route through Alembic's workshop only after it moves to Distillery.

- **D12, fetch the pinned MiniLM for S3 (2026-11-05):** Mark: "Yes, download
  it". The pinned all-MiniLM-L6-v2 artifacts live at
  `Code/testing/eidetic/models/all-MiniLM-L6-v2` (sha256 matches
  `MINILM_ARTIFACTS`), for S3's real-model run and threshold calibration.

  Links: area recall · plans P4 · summary S3 runs on the pinned MiniLM artifacts kept outside the repository.

- **D13, the goal surface moves into graphshell-mere (2026-11-05):** Mark:
  "Idk, 2, but i feel like we're gonna revisit these boundaries later anyway".
  P4's native host is the ordinary `graphshell-mere` viewer, through the
  lightest retained-text read over the resident door; the boundary is expected
  to be revisited, so no general retained-content API is built for it.

  Links: area recall · amends memory_gates D10 · plans P4 · summary The goal surface moves into graphshell-mere through a minimal resident-door read of retained text.

- **D14, close P4's remaining gaps (2026-11-05):** Mark chose all four: S1
  polish (stop words, one snippet per passage), a run over real retained
  content, the model path in the surface, and tag suggestions that survive a
  restart.

  Links: area recall · plans P4 · summary P4's remaining gaps are S1 polish, a real-content run, the model path in the surface and durable tag suggestions.

- **D15, the real-content goal run uses the browser mere (2026-11-05):**
  Mark: "Use the browser mere". Real retained captures land in Graphshell's
  extension IndexedDB, so the real-content goal surface and receipt are taken
  there; the native receipt stays on its fixture.

  Links: area recall · amends memory_gates D13 · plans P4 P5 · summary The real-content goal run is taken in Graphshell's browser mere over its IndexedDB captures.

- **D16, native retained text: plan now, build after (2026-11-05):** Mark:
  "Plan it now, build after". The native equivalent (S6) is planned here and
  built after the browser-mere goal surface lands.

  Links: area recall · plans P2 P3 P4 · summary Native retained text in Mere is planned as S6 and built after the browser goal surface.

- **D17, promote Turnstone's capture core into Eidetic (2026-11-05):** Mark
  asked "Why wouldn't we promote turnstone's to the stack?", then chose "Yes:
  core to Eidetic". S6 promotes `trail_memory`'s storage-agnostic capture core
  into Eidetic instead of writing a second native writer in Mere.

  Links: area recall · amends memory_gates D16 · plans S6 P2 P3 · summary Native retained text comes from Turnstone's capture core promoted into Eidetic.

## Findings

Verified 2026-11-01 against the current tree unless marked otherwise.

- The embedding envelope exists: `EmbeddingEnvelopeV1` at
  `crates/eidetic-core/src/models/embedding.rs:113`, `ensure_compatible` at `:126`;
  ESP's `compatibility_space()` at `esp/crates/esp/src/embed/provider.rs:75,117,156`.
- Stemma keeps entries, visits and owners as distinct arena keys
  (`crates/chartulary/src/stemma/mod.rs:60-62`). `visit_entry` takes the
  owner's current visit as parent and moves the cursor
  (`crates/chartulary/src/stemma/mutations.rs:99-125`); `admit_visit` admits a
  historical occurrence with an explicit parent and leaves live cursors alone
  (`mutations.rs:163`).
- `ImportedHistoryVisitItem` carries `source_scope`, `parent` and
  `visited_at_unix_micros` (`crates/eidetic-core/src/import/mod.rs:136-156`), but
  `history_event` still produces a URL-based `TraceEvent` with no source handle
  (`crates/eidetic-core/src/import/history.rs:55-81`).
- `verify_required_members` checks direct required members and does not walk a
  recursive dependency set (`crates/eidetic-core/src/bundle.rs:144-174`).
  `BlobManifest` records integrity, schema, source, provenance, privacy and trust
  metadata (`crates/eidetic-core/src/manifest.rs:75`).
- Muniment transactions: Redb `crates/muniment/src/redb_backend.rs:150`, directory
  `directory_backend.rs:906`, IndexedDB `indexeddb_backend.rs:261` (feature
  `indexeddb`, `crates/muniment/Cargo.toml:111`). The trait default refuses with
  `StoreError::NotTransactional` (`backend.rs:139-146`), which Fjall inherits
  (`fjall_backend.rs` has no override); ZIP refuses explicitly
  (`zip_backend.rs:299-301`). An IndexedDB implementation or a browser compile
  pass is not a headed durability or quota receipt.
- Lexical recall is an in-tree weighted BM25 (`crates/eidetic-search/src/lib.rs:20`,
  `bm25.rs`); no manifest in the workspace depends on Tantivy.
- Structured records live in `crates/eidetic-core/src/records/` and
  `crates/eidetic-search/src/records/`; comparison and query in
  `crates/eidetic-search/src/{compare,query,recall}.rs`. The archived plan's
  file-ownership table is a 2026-10-26 proposal, not this layout.
- Fleece's portable-clip check refuses schema-bearing parts
  (`mere/crates/fleece/src/portable_clip.rs:88`).
- Community collections retain causal membership over signed contribution
  references (`gemot/crates/gemot/src/moot/records/collection.rs:18-34`); that
  authority is distinct from an arbitrary private collection's selection and
  admission.
- Alembic source selectors use UTF-8 byte offsets; Eidetic passage selectors use
  Unicode scalar offsets (archived plan, M2 findings, 2026-10-29).
- Turnstone's `semantic_challenge_20261026` records 8/15 semantic passes against
  15/15 structural passes, over synthetic PageText sources; neither that failure
  nor the four-case success is a verdict on all models. `SemanticReview::Unreviewed`
  stays for existing model judgments (archived plan, M1 findings, 2026-10-26).
- 2026-11-01: the dead citation `Code/output/graphshell-components/validation-resumed/`
  in the archived plan's 2026-10-26 Progress entry no longer exists in the
  workspace; it stays there as history.
- 2026-11-05: browser writers are Graphshell extension pages only; the MV3
  service worker queues in `chrome.storage.local` and opens no IndexedDB. A
  host Web Lock (`mere/ports/graphshell/web/extension-profile.js:27`,
  `claimWriter`) already makes one page the writer and the rest followers;
  followers do not yet hold `IndexedDbBackend::read_only()`. See
  `../research/2026-11-05_browser_storage_posture_brief.md`.
- 2026-11-05: `IndexedDbBackend::list`/`scan` narrow `getAllKeys` with an
  `IDBKeyRange` when a bound is made of characters below U+D800, where UTF-16
  and code-point order agree (`crates/muniment/src/key_range.rs`); otherwise
  they enumerate every key. Results are post-filtered in Rust either way.
  `transact` still snapshots the whole store per call.
- 2026-11-05: in production, neither Mere's resident nor graphshell-mere
  opens a `PageTextStore` or `ResourceCaptureStore`; the opens are the goal
  surface fixture and proof host, `capture_resources.rs` tests and
  `mere-resident` tests that lend a `PageTextCorpus`. The resident door
  (`chirograph` `CarrierRequestBody`) has no data-returning read for retained
  text; `IntentResult` carries no payload. Turnstone's `trail_memory` writes
  both stores natively over Fjall.
- 2026-11-05, S6 inventory of Turnstone `src/trail_memory/` (about 14.5k
  lines): about 3.6k lines of stack core (1.7k clean: `sealed_capture`,
  `sealed_records`, `capture_source_document_in`, `verify_source_document`,
  `preflight_source_artifact`, `recall_index_mint`, `page_corpus`; 1.9k
  entangled with Turnstone types: `saved_sources`, the `*_receipts` and
  `emit_*_recovery` files) plus about 3.6k of tests that move with it; about
  5.7k Turnstone-specific (worker, `record_actor`, `open_memory`, M5
  inspection and inventory, inference owners, `structured_bounded_store`).
  No `Backend::transact` use: it relies on ordered apply, idempotent writes
  and receipt-then-recover. Seams: `muniment::Backend`, Eidetic's existing
  `PayloadSealer` (`crates/eidetic-core/src/seal.rs:93`, so no Dramatis
  dependency), a clock trait, Eidetic-owned neutral request and document
  types, and a capture error over `StoreError`. Thin wrappers (`page_corpus`,
  `keep_page_text`, `navigation`) collapse into `eidetic::browsing` rather
  than being copied. Risks: stored bytes (sealed markers, receipt envelopes,
  key layouts) must stay readable, so freeze golden fixtures from a real
  session first; it is proven only on Fjall, so the fault-at-each-write suite
  runs on the reservoir too; and two writers for the same keys if it is not
  reconciled with `BrowsingMemory`/`PageTextStore`. Turnstone repins from
  Eidetic `1fcdb359` after the landing.
- 2026-11-05: Fleece's `run_traversal` bounds nodes, depth, fan-out and
  pending candidates (`mere/crates/fleece/src/traversal.rs:41-49`); it has no
  request or byte limit and its `GoalDecision` carries no explanation, which
  S1's `GoalRecord` supplies.
- 2026-11-05: redb 2.6.3 creates v2 files by default
  (`Builder::create_with_file_format_v3`, `db.rs:1149`); `Database::upgrade`
  (`db.rs:459`) rewrites the commit headers in place with no documented crash
  atomicity; the format has no public reader, so Muniment reads the primary
  commit slot's version byte (`header.rs:44-76`). redb 2.6.3 panics instead of
  erring on some damaged files at open (`page_manager.rs:243`,
  `region.rs:333`); Muniment catches that as corruption. Every consumer in the
  family opens Muniment's redb files through `RedbBackend`; gemot declares an
  unused `redb = "2"` (`crates/gemot/Cargo.toml:49`) that phase 2 must drop.
- 2026-11-01: the archived plan's browser-intake source findings (latest-only
  visit selection, same-URL window dropping, Firefox exporter precision, transition
  vocabulary) were repair targets of `8c490e23` and are not carried; only the
  recall-trace and custody gaps above are confirmed open.

## Progress

- 2026-11-01: cut from the Eidetic development plan (D1). The archived copy
  carries a Superseded header, marks the embedding envelope, the P4 first
  done-clause and the M4 storage extension as landed, annotates the 2026-10-26
  priority line as superseded by D2, and repoints its links to the extracted
  crates into this repository and to the `esp` and `gemot` repositories. The
  M1-M5 run outputs moved to `Code/testing/eidetic/` (D3). Draft, unreviewed.
- 2026-11-05: P4 S1 landed: `crates/eidetic-search/src/goal.rs`, ten focused
  tests, wasm32 check. P6 storage: IndexedDB `list`/`scan` key ranges
  (`d2ebdf3`, bound arithmetic tested natively; no browser run) and crate docs
  corrected to say IndexedDB is the shipped browser backend and OPFS is not
  (`496ad9d`). P4 S4's Eidetic half landed (`c96092d`):
  `records::suggestions` holds `TagSuggestionV1` (id = BLAKE3 over the whole
  record, so a regeneration is a new suggestion), the append-only
  `TagSuggestionReviewV1` (latest review wins; a later reject does not
  retract an accepted draft), `PersonalTagAssertionDraftV1` citing the
  suggestion, `KeepingTier` (runtime, rebuildable, retained) and a query
  helper, with schemas seeded by `bootstrap_record_schemas`. Eidetic-search's
  `TagId` is now `records::TagIdentity`. Admission of the draft into the owning
  graph is open.
- 2026-11-05: D8 phase 1 landed (`26c5efa`, `0cd9047`): `RedbBackend::open`
  creates format v3 and upgrades a v2 file on open after a durable
  `<file>.pre-v3` backup, restoring it if the upgrade fails;
  `open_reporting` returns `RedbOpenOutcome` (created, current, upgraded).
  Interrupted-upgrade cases are fault-tested with injected failures, not a real
  crash. Phase 2, the crate bump past redb 2.x, waits until consumers have
  opened their files with this release.
- 2026-11-05: P4 S2 and S4's Mere half built on Mere branch `agent/p4-s2`
  (`c03040746`, `f99169bee`), unmerged per D9. Fleece feature
  `retained-traversal`: `run_retained_traversal` drives `run_traversal` with
  S1's selector and assessor over a host `RetainedProvider` that never fetches,
  refuses version or source mismatches, enforces request and byte budgets
  through the cancel seam (`RetainedStop::RequestBudget`/`ByteBudget`), and
  emits a serializable receipt with the `GoalRecord`; 13 tests, wasm32 check.
  Graph-kernel `admit_accepted_tag_suggestion` writes a personal `taggedWith`
  citing the suggestion as basis evidence `urn:eidetic:tag-suggestion:<hex>`
  under method `urn:mere:method:accepted-tag-suggestion`; idempotent per
  asserter; 7 tests. The host maps `RecallSourceId` to a resource and the
  reviewer string to an agent IRI. Both checked against Eidetic `8b4b3ec`.
- 2026-11-05: D6 built on Mere branch `agent/follower-queue` (`dc2a5d13d`,
  `7fde6d7c6`, `a69301d58`), unmerged per D9. A Graphshell follower opens its
  store with `IndexedDbBackend::read_only()` and queues adds, titles and tags
  to the writer, which admits them through its own save funnels, refuses stale
  or changed bases with a reason, and replays a repeated request id's answer.
  Checked by Graphshell lib tests, the writer-handover smoke and a partial
  headed Chromium run; not qualified. Its detail and open items are in Mere's
  application experience plan on that branch.
- 2026-11-05: IndexedDB lifecycle (`aea66e8`): an open connection closes
  itself on `versionchange` and later calls fail with a reopen error
  (`IndexedDbBackend::is_closed_for_upgrade`); an open that is `blocked` by an
  older connection fails instead of hanging. Pure logic tested natively;
  browser events unexercised. Dormant until `DATABASE_VERSION` moves past 1.
- 2026-11-05: `agent/p4-s2` and `agent/follower-queue` merged into Mere main
  by the release-log lane after Mere moved to Eidetic `5182e4e`; a hidden
  writer's queued edits now drain without animation frames (`8131e527a`).
  Fresh clone of Mere `d952835f9`: mere-fleece (retained-traversal) 20,
  mere-kernel 535, graphshell 330 pass.
- 2026-11-05: P4 S3 built, stub-only (`365f569`):
  `eidetic_search::goal_model::ModelGoalAssessor` wraps S1 over a
  `GoalEmbedder` (query and document roles), records S1's `GoalRecord` beside
  per-step model scores, the combination rule and an `EmbeddingEnvelopeV1`
  naming the space, the `eidetic.goal-chunk/v1` chunker and the sources read.
  Selection defaults to model ranking; assessment defaults to S1-or-model
  (similarity at least 500 thousandths, to be calibrated per space); an
  embedder failure falls back to S1 for that step and is recorded.
  `EspGoalEmbedder` (features `bert`/`recall-example`; `bert` now enables
  ESP's `persistence` for `compatibility_space`) compiles but has not run a
  real model, so S3 does not yet count toward P4's done-clause: that needs a
  local BERT model, a calibrated threshold and a run through Fleece's
  `run_retained_traversal`.
- 2026-11-05: `PassageSelector::to_byte_range`/`from_byte_range`
  (`models::selector_units`, `d2a8673`) convert scalar selectors to UTF-8 byte
  ranges and back against the exact text, refusing reversed, out-of-bounds and
  mid-sequence offsets.
- 2026-11-05: S3's first real-model run (`067e295`): pinned all-MiniLM-L6-v2
  through `EspGoalEmbedder` on CPU (`esp.bert-f32`, 384 dimensions, no query or
  document prefix). On a synthetic, hand-labelled corpus of 8 goals and 56
  sources (`crates/eidetic-search/tests/fixtures/goal_calibration/`), S1 finds
  no paraphrases (0/16) and fires on every near-miss; the `Or` rule at 220
  thousandths lifts recall from 0.50 to 1.00 and F1 from 0.51 to 0.80
  (precision 0.67), on a flat F1 plateau from 140 to 290; the generic 500
  default gives recall 0.56. Near-misses (195 to 716) overlap paraphrases
  (299 to 536), so no cosine threshold restores precision.
  `recommended_match_similarity_milli` returns 220 for the pinned weights,
  opt-in; the default stays 500. Recalibrate on real retained sources before
  relying on it. Debug CPU: load 1.7 s, 56 pairs 51 to 78 s.
- 2026-11-05: P4 S5, lexical path, merged into Mere main (`ea317d523`,
  `0b806729d`, merge `f23ddcc0e`). Graphshell feature `goal-surface`: a
  `RetainedProvider` over a snapshot of `PageTextStore` and
  `ResourceCaptureStore` (source = chartulary resource id, version = BLAKE3 of
  the page text, links from `projected_relations`), a native goal surface
  with cited passages and a per-step "how the walk chose" explanation, view
  state only; 9 tests. Headed receipt
  `mere/ports/graphshell/docs/receipts/goal_surface_20261105/` (Apple M4,
  macOS 26.5.1, Eidetic `5182e4e`, captures under
  `Code/testing/eidetic/goal_surface_20261105/`), checked by eye: two cited
  results with versions, terms and highlighted passages, and the
  explanation including an unretained target reported as not fetched.
  Limits: synthetic five-page fixture, harness-driven input, the port's
  shared native host rather than `graphshell-mere` (whose door serves
  projection cards, not retained text), no model path, no proposals (D11).
  Seen in the capture: S1 counts the stop word "a" as a goal term, and each
  matched term gets its own snippet of the same sentence.
- 2026-11-05: S1 polish (`8960601`): goal terms drop a 63-word English stop
  list (`eidetic.stop-words/en-v1`, recorded in `GoalRecord`) unless the goal
  is only stop words; `GoalAssessment.passages` merges hits within 40 scalars
  and adds 60 of context; operation `eidetic.goal-assessment/v2`. Tag
  suggestions are durable (`4a6c3c3`): `records::SuggestionLedger` journals
  suggestions and reviews through Muniment (runtime tier in memory only),
  reopened on Redb and directory backends in tests. Lexical producer
  (`cb90383`): `eidetic_search::suggest_tags` proposes concepts whose label
  terms all occur in a source, with evidence selectors.
