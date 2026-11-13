# Portolan plan: split and rename chartulary

**Status (2026-11-12):** in progress. The twelve forks are answered (D3 to D15);
P0 is done and P1 is being built in the `eidetic-portolan` worktree, unpushed.

Eidetic's `chartulary` crate (`crates/chartulary`, 10,190 source lines in 19
modules, plus a 157-line example) becomes four crates. The core is renamed
`portolan`, the `stemma` module becomes the crate `hodograph`, `quads` becomes
`quadstore`, and the fields modules become `fieldbook`. This
plan measures what crosses each seam, who consumes what, what is persisted and
what is published, then lays out the phases.

All measurements are at origin/main on 2026-11-12: Eidetic `1371574`, Mere
`fa938d92c`, and every repository under `Code/repos` that names chartulary.
Seiche and Inker are not checked out; their manifests and sources were read
through the GitHub API at `main`. Other repositories that exist only on GitHub
were not checked.

## Decisions

- **portolan D1, split chartulary (Mark, 2026-11-12):** "i'm down to split
  this." The core, `stemma`, `quads` and the fields modules leave one crate
  for four.

  Links: area The stack · plans portolan · summary chartulary splits into a core and three leaf crates.

- **portolan D2, the names portolan and hodograph (Mark, 2026-11-12):** "sure.
  portolan and hodograph are good. i don't mind renaming." The core is
  `portolan`; the lineage layer (`stemma`) is `hodograph`. Both were free on
  crates.io on 2026-11-12. Names are safe only once really published.

  Links: area The stack · draws on portolan D1 · plans portolan · summary The core is portolan and the lineage crate is hodograph.

- **portolan D3, release the old name (Mark, 2026-11-12):** "i really meant
  'release the name of chartulary back to the crates.io pool'". The aim is to
  delete `chartulary` from crates.io, not keep it as a pointer. Carried by D11
  and P6.

  Links: area The stack · draws on portolan D2 · plans portolan · summary chartulary's crates.io name goes back to the pool once nothing published needs it.

- **portolan D4, F1, the quads crate is quadstore (Mark, 2026-11-12):**
  "quadstore", chosen over `graticule` (recommended) and `quaternio`.

  Links: area The stack · draws on portolan D1 · plans portolan · summary The quads crate is quadstore.

- **portolan D5, F2, the fields crate is fieldbook (Mark, 2026-11-12):**
  "fieldbook (Recommended)".

  Links: area The stack · draws on portolan D1 · plans portolan · summary The fields crate is fieldbook.

The answers D6 to D15 are the coordinator's, told to Mark; each is his to
overrule.

- **portolan D6, F3 (coordinator, 2026-11-12):** (A). `canonical` is a module of
  portolan, with the root re-exports kept.

  Links: area The stack · plans portolan · summary canonical stays in portolan.

- **portolan D7, F4 (coordinator, 2026-11-12):** (A). `rdf` is a module of
  portolan. The quads test that uses `rdf::Term` moves to portolan, with
  quadstore as a dev-dependency, so quadstore has no dependencies.

  Links: area The stack · plans portolan · summary rdf stays in portolan; quadstore has no dependencies.

- **portolan D8, F5 (coordinator, 2026-11-12):** (A). A transitional
  `chartulary` shim, git-only and never published; its removal is P4's
  done-condition.

  Links: area The stack · plans portolan · summary A git-only chartulary shim carries consumers until P4 removes it.

- **portolan D9, F6 (coordinator, 2026-11-12):** (A). The flip path and the
  meta fields move unchanged; revisit after Mere P6's derived-layer benchmark.

  Links: area The stack · draws on mere:quad-index plan · plans portolan · summary quadstore's API moves unchanged.

- **portolan D10, F7 (coordinator, 2026-11-12):** (A). `TransitionKind` moves
  unchanged into hodograph; generalizing it is a later plan.

  Links: area The stack · plans portolan · summary TransitionKind moves unchanged.

- **portolan D11, F8 (coordinator, 2026-11-12):** no further chartulary
  releases and no published shim. The aim is to delete the crate from
  crates.io once its 11 published dependents have each published a version off
  it; P6, which first confirms crates.io's deletion rule.

  Links: area The stack · draws on portolan D3 · plans portolan · summary No more chartulary releases; deletion once dependents move off it.

- **portolan D12, F9 (coordinator, 2026-11-12):** (A). The three persisted
  `chartulary.class` strings stay verbatim.

  Links: area The stack · plans portolan · summary Persisted chartulary.* identifiers stay as written.

- **portolan D13, F10 (coordinator, 2026-11-12):** (A). All four crates start
  at 0.1.0.

  Links: area The stack · plans portolan · summary The four crates start at 0.1.0.

- **portolan D14, F11 (coordinator, 2026-11-12):** (A). The G0 to G3 doc file
  names are kept; their text is updated.

  Links: area docs · plans portolan · summary G-doc file names kept, text updated.

- **portolan D15, F12 (coordinator, 2026-11-12):** (A). Publish right after P1
  lands on Eidetic main; publishing is Mark's to run.

  Links: area The stack · plans portolan · summary Publish right after P1 lands; Mark runs it.

## Findings (2026-11-12)

### F-1. The seams are cleaner than assumed

Every `crate::` reference in `crates/chartulary/src` was read. What crosses
each proposed seam, item by item:

| From | To | What crosses | Kind |
|---|---|---|---|
| `stemma` (5 files) | anything in the crate | nothing | none: it uses only serde, slotmap, rkyv and std |
| `quads` | `rdf` | `rdf::Term`, in one test (`quads/tests.rs:312`, `the_rdf_projection_interns_and_indexes`) | test only |
| `quads` | core | `Container`, `Graph`, `Recognized`, `Relation`, `RelationClass`, in the same test (`quads/tests.rs:313`) | test only |
| `quads/mod.rs:16` | `rdf` | an intra-doc link to `crate::rdf::Term` | doc only |
| `coupling` | `taxonomy` | an intra-doc link, `coupling.rs:51`: "mirrors [`crate::taxonomy`]'s `REL_VOCAB`" (no `REL_VOCAB` exists; taxonomy has `REL_NS`) | doc only |
| `edge_path` | `field` | `field::FieldId` (`edge_path.rs:22`) | code |
| `field` | `field_ast` | `ScalarField`, `VectorField` (and `Falloff` in tests) | code |
| `field_ast` | `field` | `FieldId` | code |
| `coupling` | `field` | `CouplingId`, `FieldId` | code |
| `taxonomy` | `rdf` | intra-doc links only (`taxonomy.rs:21,27`) | doc only |
| `rdf` | core | `Addressed`, `Graph`, `Identified`, `Labeled`, `NodeKey`, `Predicated` (`rdf.rs:30`); tests add `Container`, `Recognized`, `Relation`, `RelationClass` | code |
| `canonical` | anything | nothing | none: uuid only |
| core modules | `canonical`, `rdf`, `stemma`, `quads`, fields | nothing | none |

Three corrections to the picture this plan started from:

1. **`rdf` depends on the core, not the other way round.** Taxonomy names `rdf`
   only in doc links. `rdf` imports six core items. So `rdf` cannot go into the
   quadstore without making quadstore depend on the core.
2. **`quads` has no code dependency on anything.** Its one use of `rdf` and the
   core is a single test. With that test moved, quadstore has zero
   dependencies, normal or dev.
3. **`coupling` has no code dependency on `taxonomy`.** The one "use" is a doc
   link to a constant that does not exist. `edge_path` must go with fieldbook,
   since it imports `FieldId`.

No module is feature-gated. The only cfg gates are in `field.rs`
(`FieldId::new` and `CouplingId::new` are `cfg(not(target_arch = "wasm32"))`)
and the uuid target tables in the manifest.

### F-2. Sizes and external dependencies per target crate

The `rkyv` and `petgraph` mentions in `field.rs`, `field_ast.rs`, `coupling.rs`
and `edge_path.rs` are comments ("rkyv lands at the `Persisted*` DTO layer").

| Target crate | Modules | Source lines | Normal dependencies | Dev dependencies |
|---|---|---|---|---|
| `portolan` | assertion, caps, commit (+ tests), container, content_class, edit, facet, graph, nested, spine (+ materialize, tests), taxonomy; `lib.rs` | 5,354 (1,432 of them test files) | muniment (workspace, no default features), petgraph 0.8 `serde-1`, serde `derive`, serde_json | pollster 0.4, muniment `json` |
| + `canonical` if it stays (F3) | canonical | 263 | adds uuid 1 `v5` | none |
| + `rdf` if it stays (F4) | rdf | 346 | none new (serde_json) | none |
| `hodograph` | stemma (mod, mutations, queries, snapshot, tests) | 1,902 | serde `derive`, slotmap 1 `serde`, rkyv 0.8 `std` | none |
| `quadstore` | quads (mod, dictionary, index, tests, apply_tests); `examples/quads_apply.rs` | 1,382 (+157) | none | none, once the rdf test moves (F4) |
| `fieldbook` | field, field_ast, coupling, edge_path | 943 | serde `derive`, uuid 1 `serde`, `v4` off wasm32, `js` on wasm32-unknown-unknown | serde_json |

The core needs no rkyv, slotmap or uuid today. rkyv and slotmap leave it with
hodograph. uuid stays in it only if `canonical` does (F3); muniment does not
depend on uuid.

### F-3. Consumers

Fourteen repositories name chartulary at origin/main: eleven local ones plus
Seiche and Inker (remote), with Eidetic itself. Isocosm, mer3ly
(`merelyllc.com.git`) and Inker carry patch rows but import nothing; armillary,
genet, netrender and dramatis mention it only in text. The import scan expanded every `chartulary::` path, brace groups
included, and mapped root re-exports back to their modules.

Manifest rows. "Dep" is a `[dependencies]`, `[dev-dependencies]` or
`[workspace.dependencies]` row; "patch" is a `[patch.*]` row. Feature lines
(`"dep:chartulary"`) are in five manifests, marked *f*.

| Repo | Manifests | Dep rows | Patch rows | Notes |
|---|---|---|---|---|
| cleromancy | 2 (`Cargo.toml`, `core/Cargo.toml`) | 2 | 2 | `core` compiles the root `src/` (`path = "../src/lib.rs"`) |
| esp (`distillery.git`) | 4 | 2 (`crates/athanor` dep *f*, its workspace row) | 3 | |
| gemot | 6 | 3 (workspace, `commons`, `gemot` dev) | 5 | two patch rows target `mere.git`, which no longer carries chartulary |
| graphshell | 7 | 6 (`host` workspace, `cambium`, `cartography` *f*, `graphshell-host` *f*, `mere-resident`, `moot` dev) | 1 | |
| isocosm | 4 | 0 | 4 | patch only |
| knot-editor | 3 | 2 | 2 | |
| mer3ly | 2 | 0 | 2 | patch only |
| mere | 5 | 5 (workspace `version = "0.3.1"`, `personal-facts`, `engine`, `graph-kernel`, `pandect`) | 0 | |
| murm | 2 | 1 (`signalman-desktop`) | 1 | |
| turnstone | 3 | 3 (root dep, workspace, `ports/graphshell` *f*) | 2 | |
| woodshed | 5 | 2 (`woodshed-core`, `woodshed-graph`) | 3 | |
| seiche | 1 (`crates/conatus/numen`) | 1 | 0 | |
| inker | 1 (root) | 0 | 1 | patch only |
| **13 consumer repos** | **45** | **27** | **26** | |
| eidetic | 4 (root, `chartulary`, `eidetic-core`, `servitor`) | 3 | 2 | moves inside P1 |

Each patch row either redirects a registry `chartulary` that a published family
crate pulls in, or is unused; a build says which (cargo warns "patch was not
used"). Patch rows can redirect a registry `chartulary`, never point it at
`portolan`.

Imported items, grouped by the crate they would come from. Files are `.rs`
files that import chartulary, by owning crate.

| Repo | Crate (files) | portolan | canonical | hodograph | quadstore | fieldbook | rdf |
|---|---|---|---|---|---|---|---|
| cleromancy | root (8) | AcceptAll, Container, EditSpec, FacetError, FacetId, GraphLog, Relation | | | | | |
| esp | athanor (2) | AcceptAll, FacetId | canonical_url, resource_id_from_canonical_iri | | | | |
| gemot | commons (7), gemot (1, dev) | Author, Batch, Container, EdgeId, EditSpec, FacetId, GraphEdit, GraphLog, Identified, Relation, WriterId, taxonomy::{Recognized, RelationClass} | | | | | |
| graphshell | cambium (1) | | | Stemma, StemmaSnapshot, EntrySnapshot, OwnerSnapshot, VisitSnapshot, BindingSnapshot, EntryPrivacy, VisitId, TransitionKind | | | |
| graphshell | cartography (8) | (1 file) | resource_id (1 file) | | | field::{FieldDefinition, FieldId}, field_ast::{ScalarField, VectorField}, coupling::CouplingResponse (6 files) | |
| graphshell | graphshell-host (3), mere-resident (1), moot (1, dev) | AcceptAll, Author, Container, FacetError, FacetId, Graph, Identified, Labeled, Recognized, Relation, RelationClass (all graphshell) | resource_id, resource_id_from_canonical_iri | | | | |
| knot-editor | knot-editor (6) | AcceptAll, Addressed, CLASS_FACET, ClassId, ClassMembership, ClassRegistry, Container, ContentClass, EditSpec, FacetId, FacetStore, FacetValidator, GraphLog, Labeled, NodeFacets, Relation | resource_id | | | | |
| mere | personal-facts (1) | (core items) | | | | | |
| mere | engine (6) | (core items) | (1 file) | | Dictionary, Quad, QuadIndex, QuadMeta, TermId, View (3 files) | | |
| mere | graph-kernel (27) | 19 files | 12 files | 5 files | | 1 file (`graph/mod.rs`, re-exports) | |
| mere | pandect (17) | 11 files | 5 files | 1 file (`addressable_graph/schema.rs`) | | | |
| mere | all crates | AcceptAll, Address, Addressed, Author, Classified, Container, ContainerAddress, ContentBearing, ExpiringFacet, FacetError, FacetId, FacetStore, FacetValidator, Graph, GraphBearing, GraphLog, Identified, Labeled, NodeFacets, Predicated, Relation, RelationClass | canonical_url, resource_id, resource_id_from_canonical_iri | Stemma, StemmaSnapshot, EntrySnapshot, OwnerSnapshot, VisitSnapshot, EntryPrivacy, TransitionKind | as above | COUPLING_VOCAB, Coupling, CouplingResponse, NodeSelector, EdgePath, EdgePathRule, CouplingId, Field, FieldDefinition, FieldExtent, FieldId, FieldLifecycle, Falloff, ScalarField, VectorField | |
| murm | signalman-desktop (1) | Author, EdgeId, EditSpec, GraphLog, Identified, NodeKey | | | | | |
| turnstone | root (43) | AcceptAll, Author, Batch, CLASS_FACET, ClassError, ClassMembership, ClassRegistry, Container, ContentClass, EditSpec, FacetError, FacetId, GraphLog, Relation | 4 files | | | | |
| turnstone | ports/graphshell (16) | 6 files | 12 files | TransitionKind (1 file, `capture.rs`) | | | |
| woodshed | woodshed-core (2) | AssertionBasis, Author, EdgeId, Relation, RelationClass (re-exported) | | Stemma, StemmaSnapshot, EntryPrivacy, TransitionKind | | | |
| woodshed | woodshed-graph (2) | AssertionMethod, AssertionRecord, Batch, CommitError, Container, EditSpec, GraphLog, LogId | | (tests) | | | SCHEMA_KEYWORDS, SCHEMA_NAME, Term::Literal, to_quads (in `src/tests.rs`) |
| seiche | numen (1) | | | | | coupling, edge_path, field, field_ast, all re-exported (`numen/src/lib.rs:50-56`) | |
| eidetic | eidetic-core (7) | AssertionBasis, Author, CommitError, Committed, Container, ContentClass, EdgeId, EditSpec, FacetError, FacetId, FacetValidator, GraphLog, Relation, RelationClass (1 file) | canonical_url, resource_id (4 files) | Stemma, StemmaSnapshot, EntrySnapshot, OwnerSnapshot, VisitSnapshot, BindingSnapshot, EntryIdentityKey, MemoryPayload, OwnerIdentity, EntryPrivacy, TransitionKind, TransitionRecord (2 files) | | | |
| eidetic | servitor (3) | (core items) | | | | | |

Totals: 163 local `.rs` files plus Seiche's one. By target: 113 files touch the
core, 43 `canonical`, 12 stemma, 7 fields, 3 quads, 1 `rdf`. Nine files in Mere
and three in Turnstone touch more than one target.

Every one of the 10 crates that imports `canonical` also imports core items.
`canonical` is not 70% of what Mere and Graphshell import by any measure taken
here: by files it is 18 of Mere's 51 and 2 of Graphshell's 14; by use sites it
is 17% in Mere and 5% in Graphshell (an upper bound on the core's share, since
names like `Container` and `Graph` also match local types).

Consumers that re-export chartulary items, so their own dependents need no
change: Mere `engine` (`quads::View`), `graph-kernel` (`TransitionKind as
NodeHistoryTransitionKind`, and the whole fields surface at `graph/mod.rs:222-227`),
`pandect` (`facet_store.rs:46`), Seiche `numen`, woodshed-core
(`personal_relations.rs:15`), eidetic-core (`canonical_url`, `resource_id`).

One crate gets lighter: Graphshell's `cambium` uses only stemma items, so it
would depend on `hodograph` alone and drop petgraph and muniment. Seiche's
`numen` would depend on fieldbook alone.

Names in live non-Rust files: Genet's CI family list
(`support/ci/check_dependency_cones.py:295`), Turnstone's
`.cargo/config.toml.example:74` (already stale: it points into Mere), the
`deny.toml` reasons in dramatis, esp, isocosm, mere and eidetic, and Mere's
`support/family-promotion/` scripts (historical). Several hundred receipt and
log files name chartulary; they are history and stay as written.

### F-4. Publish state (crates.io API, 2026-11-12)

- `chartulary`: 0.1.0, 0.1.1, 0.1.2, 0.2.0, 0.2.1, 0.3.0 (0.3.0 published
  2026-11-03). Eidetic main is at 0.3.1, unpublished. `quads`, `field`,
  `field_ast`, `coupling` and `edge_path` arrived after 0.3.0 (2026-11-09 and
  2026-11-10), so no published version has them.
- Published reverse dependencies of chartulary (11): cambium 0.6.1 (`^0.2.1`),
  eidetic 0.0.3 (`^0.3.0`), gemot 0.1.1 (`^0.2.0`, dev), graphshell 0.0.2
  (`^0.2.1`, optional), knot-editor 0.0.3 (`^0.2.1`), mere-eidetic 0.0.2
  (`^0.2.0`, optional), mere-kernel 0.0.3 (`^0.3.0`), mere-signals 0.0.2
  (`^0.2.1`), pandect 0.2.0 (`^0.3.0`), scholia 0.1.1 (`^0.1.0`), servitor 0.0.3
  (`^0.3.0`).
- `muniment`: 0.2.1 published; Eidetic main is at 0.2.1. Eighteen published
  reverse dependencies, chartulary 0.3.0 among them. Portolan can publish
  against it as it stands.
- Free on crates.io: `portolan`, `hodograph`. Taken: `stemma` (a DOCX engine,
  0.7.0), `cartography` (engine split plan, 2026-11-07 finding).

### F-5. Persisted names

- **The crate and module paths are in no persisted bytes.** No `type_name`,
  `module_path!` or schema id in any consumer is built from a chartulary path.
  Chartulary has no `#[serde(tag)]`, `#[serde(rename)]` or rkyv attribute that
  names a path. serde writes variant and field names, not type paths; rkyv
  writes neither. `canonical::resource_id` hashes under the standard
  `Uuid::NAMESPACE_URL`, not a crate-derived namespace.
- **Three strings carry the old name and are data:**
  `CLASS_FACET = "chartulary.class"` (`content_class.rs:33`), the facet id a
  node's class membership is stored under; it is written into journals through
  `GraphEdit::SetFacet { facet: FacetId, .. }` (`edit.rs:198-203`), and used by
  Knot (`content_classes.rs`, `directory.rs`) and Turnstone
  (`content_classes.rs`). The schema id `"chartulary.class/v1"` (Knot
  `content_classes.rs:57`, Turnstone `content_classes.rs:97`) and
  `"chartulary.ContentClass/v1"` (eidetic-core `schema_facets.rs:111`). See F9.
- **Pandect hashes stemma snapshots.** `pandect/src/addressable_graph/schema.rs`
  stores `StemmaSnapshot<String, String, NodeHistoryOwner, ()>` as serde JSON and
  checks it against a blake3 digest. Moving the type to `hodograph` changes no
  byte; renaming a `TransitionKind` variant would (F7).

### F-6. `TransitionKind`

Ten variants: `LinkClick`, `UrlTyped`, `Back`, `Forward`, `Reload`, `Redirect`,
`TabSpawn`, `Restore`, `Imported`, `Unknown`. The module doc
(`stemma/mod.rs:27-29`) calls it "the one browser-flavored remnant of its
origin" and says "generalizing it (to a type parameter or a neutral vocabulary)
is the open naming decision for this crate". The enum's own doc
(`stemma/mod.rs:117-136`, findings of 2026-07-26) adds: if generalized, make it a
type parameter, not a `String` tail, since `HashMap<TransitionKind, u64>` keys
and `Copy` reads rely on it; and app engagement kinds (Woodshed's previewed,
staged, rehearsed, completed) are not a reason, since `context: X` on
`VisitRecord` is their slot.

Users: Mere graph-kernel (8 files; `UrlTyped` 22 uses, `LinkClick` 5, `Reload`
1, `Unknown` 1; re-exported as `NodeHistoryTransitionKind`), eidetic-core
(maps all ten onto its browsing transitions), Turnstone
`ports/graphshell/src/capture.rs` (an exhaustive match over all ten, line 345),
Graphshell cambium (`Unknown`), Woodshed core and graph tests (`LinkClick`,
`Unknown`). Every variant is used.

### F-7. `QuadIndex::apply`'s flip path and `QuadMeta`'s optional fields

- The flip path (the `withdrawn` argument of `QuadIndex::apply`,
  `quads/index.rs:177`) is called only by chartulary itself:
  `examples/quads_apply.rs:134` and `quads/apply_tests.rs:187,198` (plus the
  randomized equality tests). Mere passes `[]`:
  `engine/src/dataset/refresh.rs:141`, `self.index.apply(asserted, [], removed)`.
- `QuadMeta.asserter` and `QuadMeta.statement` are `Some` only in chartulary's
  own `quads/tests.rs` (4 sites). Mere always passes `None`: `QuadMeta::HELD` and
  its `WITHDRAWN` constant (`engine/src/dataset.rs:24-27`).
- Mere's quad index plan records, 2026-11-12, "the flip path stays" (the
  removal fork answered "take (A)", relayed by the coordinator). Mere's engine
  split plan (D143, P6 derived layer) may need "a third `View` or a meta bit in
  chartulary" for derived rows. See F6 in the fork list.

### F-8. Mere is moving

Mere main took 43 commits from 2026-11-11 to 2026-11-12, many in
`graph-kernel`, `engine` and `pandect`, the three crates holding 50 of Mere's
51 chartulary-importing files. The engine split plan's P6 is the next phase.
Any Mere edit here must be a script run against fresh main at commit time, one
commit, no hand edits.

## Target layout

Illustrative, not compile-ready.

```
crates/portolan/     core modules, lib.rs re-exports; canonical and rdf per F3, F4
crates/hodograph/    stemma/{mod,mutations,queries,snapshot,tests}.rs -> src/
crates/quadstore/      quads/* -> src/, examples/quads_apply.rs
crates/fieldbook/     field.rs, field_ast.rs, coupling.rs, edge_path.rs
crates/chartulary/   transitional shim, only if F5 is (A)
```

Path changes for consumers, one table for the mechanical rewrite:

| Old path | New path |
|---|---|
| `chartulary::<core item or module>` | `portolan::<same>` |
| `chartulary::{canonical_url, resource_id, resource_id_from_canonical_iri}` | `portolan::…` if F3 is (A) |
| `chartulary::rdf::…` | `portolan::rdf::…` if F4 is (A) |
| `chartulary::stemma::X` | `hodograph::X` |
| `chartulary::quads::X` | `quadstore::X` |
| `chartulary::{field, field_ast, coupling, edge_path}::X` | `fieldbook::{field, field_ast, coupling, edge_path}::X` |
| `chartulary::{LogId, Provenance}` | `portolan::{LogId, Provenance}` (muniment re-export kept) |

The shim, if chosen (illustrative):

```rust
pub use portolan::*;
pub use hodograph as stemma;
pub use quadstore as quads;
pub use fieldbook::{coupling, edge_path, field, field_ast};
```

Its version is 0.3.2, so Mere's `version = "0.3.1"` lower bound still holds.
All four crates and the shim resolve from one `eidetic.git` revision, so the
types are the same types.

## Phases

### P0. The forks are answered

- Done when: F1 to F12 each have an answer recorded under Decisions in the
  words given; the two new names are rechecked on crates.io the day P1 starts.
- Done 2026-11-12: D3 to D15.

### P1. The split, built in a worktree

Eidetic only. `git mv` each module so history follows. Four manifests with
exactly the dependencies in F-2; workspace `members`,
`[workspace.dependencies]` and both `[patch."…eidetic.git"]` and
`[patch."…mere.git"]` rows for each new crate; the git-only shim (D8). Fix the
doc links: `coupling.rs:51`, `quads/mod.rs:16`, `taxonomy.rs:21,27`, the
crate-level docs in `lib.rs`. Move the quads rdf test to portolan (D7). Split
`README.md`. Point `eidetic-core` and `servitor` at the new crates (10 files,
2 manifests).

- Done when:
  - the chartulary test count on origin/main is recorded first, and the four
    crates plus eidetic-core and servitor pass the same tests after;
  - `cargo tree` shows: hodograph with serde, slotmap and rkyv only; quadstore
    with no dependencies; fieldbook with serde and uuid only;
    portolan with no rkyv or slotmap;
  - `cargo check --target wasm32-unknown-unknown` passes for all four (the
    uuid gates in fieldbook are target-specific);
  - `cargo publish --dry-run` passes for each of the four;
  - the commit hooks pass (line ceiling, clippy, dependency bans).

### P2. Landed and claimed

Merge P1 to Eidetic main. Publishing is Mark's to do or approve, in this order:
hodograph, quadstore, fieldbook (no family dependencies), then
portolan (needs muniment 0.2.1, already published), right after P1 lands (D15).
The `chartulary` shim is never published (D8, D11).

- Done when: the four names show on crates.io at their first versions, 0.1.0 (D13);
  the naming ledger records them, with Mark's approval for that edit.

### P3. Consumers, one small commit per repository

Each repository gets one commit: its manifest rows (renamed, plus the new rows
the tables in F-3 call for) and a scripted rewrite of `use` paths by the path
table. No hand edits. Gate only what changed: `cargo check` with all features
and the tests of each touched crate, and the wasm build where the repository
has one (Graphshell web, Turnstone web). Each repository's own push rules
apply.

Order, fewest files first, Mere last or at a quiet point:

| Order | Repo | Manifests | Files | New crates needed |
|---|---|---|---|---|
| 1 | seiche | 1 | 1 | fieldbook only |
| 2 | inker, isocosm, mer3ly | 7 | 0 | patch rows only |
| 3 | murm | 2 | 1 | portolan |
| 4 | esp | 4 | 2 | portolan |
| 5 | woodshed | 5 | 4 | portolan, hodograph |
| 6 | knot-editor | 3 | 6 | portolan |
| 7 | cleromancy | 2 | 8 | portolan |
| 8 | gemot | 6 | 8 | portolan |
| 9 | graphshell | 7 | 14 | portolan, hodograph (cambium alone), fieldbook (cartography) |
| 10 | turnstone | 3 | 59 | portolan, hodograph (ports/graphshell) |
| 11 | mere | 5 | 51 | portolan, hodograph (graph-kernel, pandect), quadstore (engine), fieldbook (graph-kernel) |

Knot first among the Mere pair if a change ever breaks both (they compile each
other). For Mere: if the P6 session holds uncommitted work in the touched files,
wait; that work is active and not ours.

- Done when, per repository: `git grep chartulary -- '*.rs' '*Cargo.toml'`
  finds nothing but comments slated for P5; its gates pass; the commit is on its
  main.

### P4. The shim retires (D8)

- Done when: no manifest in the 13 consumer repositories names `chartulary` as
  a dependency or patch, and the shim crate is removed from Eidetic in one
  commit.

### P5. Docs, pointers and the registry

DOC_README entries; the text of the G0 to G3 technical architecture docs, file
names kept (D14); the root `README.md`; Genet's CI family list; the `deny.toml`
reasons. No chartulary release, pointer or otherwise (D11). Other repositories'
design docs change only when next touched.

- Done when: Eidetic's active docs name the new crates.

### P6. The chartulary name goes back to the crates.io pool (D3, D11)

First, confirm crates.io's deletion rule from its own policy page before
anything is promised: whether a crate must have a single owner, stay under a
download ceiling or age limit, and have no reverse dependencies, and whether
old versions of dependents count as dependents. Then each of the 11 published
dependents (F-4) publishes a version off chartulary, each by its own owner's
release. Deleting is Mark's to run.

- Done when: the rule is quoted in Findings with its source and date; either
  chartulary is deleted from crates.io, or the rule is shown to forbid it and
  that finding goes back to Mark as a fork.

## What could invalidate this plan

- **A name is taken before it is published.** Only crates.io was checked. Not
  GitHub organizations, not other registries, not trademarks.
- **Two Eidetic revisions in one graph.** The shim and portolan types are equal
  only from one revision. A fresh resolve gives one; a rebuild of an old commit
  from its `house resolved` note could mix.
- **Published crates keep registry chartulary alive.** cambium 0.6.1,
  mere-kernel 0.0.3, pandect 0.2.0 and eight others name it. Its types are
  distinct from portolan's, as 0.2 and 0.3 already are; a graph where they meet
  fails to compile until the dependent republishes.
- **P6 changes the quads API.** The derived layer may add a `View` or a meta
  bit (engine split D143). That lands in quadstore during or after the
  move; it does not change the split, but a removal under F6 would collide.
- **The persisted strings change.** Renaming `chartulary.class` without a
  reader for the old id loses every stored class membership (F9).
- **`TransitionKind` changes.** A variant rename changes Pandect's hashed
  snapshot bytes (F7).
- **More moves into the core.** The Eidetic squish audit (2026-11-07) lists
  "`servitor` into `chartulary`" as a candidate. If taken, it lands in portolan.
- **Repositories not measured.** Remote-only repositories other than Seiche and
  Inker were not checked; GitHub code search is incomplete and was not used as
  evidence.

## Forks (answered 2026-11-12)

Each fork lists options, recommendation first. Answers: F1 D4, F2 D5, F3 D6,
F4 D7, F5 D8, F6 D9, F7 D10, F8 D11 (with D3), F9 D12, F10 D13, F11 D14,
F12 D15.

- **F1. quadstore's name.** It is a dictionary and six-ordering index
  over quads, 1,382 lines, with no dependencies. Checked on crates.io only:
  `graticule`, `quaternio`, `quadstore`, `quadrat`, `isogram` are free;
  `concordance`, `repertory`, `quire`, `quadrant`, `tetrad` are taken.
  (A) `graticule`: a map's grid of reference lines, which you read a position
  off by its coordinates, as a pattern reads one ordering by its bound terms.
  (B) `quaternio`: codicology's quire of four sheets; names the four terms.
  (C) `quadstore`: plain, says what it is.
  (D) keep it a module of portolan after all (undoes part of D1).
- **F2. fieldbook's name.** Field, field_ast, coupling and edge_path,
  943 lines; definitions only, evaluation stays in Seiche's numen. Free on
  crates.io: `fieldbook`, `isopleth`, `hachure`, `isoline`, `windrose`,
  `relief`; taken: `contour`. (A) `fieldbook`: a surveyor's book of field
  records; says "field" and "record, not evaluation". (B) `isopleth`: a map line
  of equal value; fits scalar fields, less so couplings. (C) `hachure`: map
  strokes showing slope; fits vector fields.
- **F3. Where `canonical` lives.** 263 lines, uuid only. All 10 crates that
  import it also import core items, so a separate crate frees none of them from
  petgraph. It strips web tracking parameters (`fbclid`, `utm_`), a browser
  concern inside a generic graph crate. (A) a module of portolan, root
  re-exports kept: no new manifest rows; portolan gains uuid `v5`. (B) its own
  crate: 10 more dependency rows, plus workspace rows; the storage layer can
  take it without petgraph. (C) into muniment: persistence takes a URL policy it
  does not use.
- **F4. Where `rdf` lives.** 346 lines; it imports six core items; one consumer
  (woodshed-graph, in `src/tests.rs`). (A) a module of portolan; the quads test
  that uses `rdf::Term` moves to portolan, which takes quadstore as a
  dev-dependency, so quadstore stays dependency-free. (B) its own crate,
  depending on portolan (the old `scholia` name is the family's on crates.io at
  0.1.1). (C) into quadstore: makes it depend on portolan.
- **F5. Shim or one pass.** Either way consumers make the same edits: 45
  manifests (53 rows) in 13 repositories and 154 files. (A) a transitional
  `chartulary` shim crate, about 40 lines, retired in P4 when no manifest names
  it: each repository moves when it is quiet and nothing goes red; precedent is
  Mere's `mere-rdf` shim, added and dropped on 2026-11-12. DOC_POLICY §3 resists
  shims, so P4's done-condition is its exit. (B) one pass, no shim: every
  consumer tracking `branch = "main"` is red from Eidetic's landing until its own
  commit, so all 13 commits go in one window, Mere's included, while P6 is
  active. (C) a permanent shim: contradicts §3.
- **F6. The quads flip path and the always-`None` fields.** The flip path's only
  callers are chartulary's example and tests; `asserter` and `statement` are
  `Some` only in chartulary's tests; Mere passes `[]` and `None`. Mere's quad
  plan ruled "the flip path stays" on 2026-11-12, and P6 may want a meta bit.
  (A) move unchanged: the move stays mechanical. (B) remove both in the move:
  contradicts the quad plan ruling, so it goes back there as a question first.
  (C) remove the two fields only, keep the flip path.
- **F7. What `TransitionKind` becomes.** Ten browser variants, all used;
  Turnstone matches all ten; Pandect hashes snapshots that contain them.
  (A) move unchanged into hodograph; its generalization stays open for a later
  plan. (B) make it a type parameter in the move, as the 2026-07-26 finding
  advises, with the browser vocabulary as the instantiation Mere, Eidetic and
  Turnstone use: no byte change, but every `Stemma` and snapshot alias gains a
  parameter (Pandect's four, cambium, Woodshed, graph-kernel). (C) a neutral
  vocabulary: renamed variants change Pandect's hashed bytes.
- **F8. `chartulary` on crates.io after the move.** Six versions, 11 published
  dependents on `^0.1` to `^0.3`, none of which would take a 0.4. Published
  versions stay forever either way. (A) a final docs-only release whose
  description and README point to the four crates. (B) publish the shim as
  0.4.0, a pointer that still compiles. (C) leave it at 0.3.0, no pointer.
- **F9. The persisted `chartulary.class` strings.** `CLASS_FACET` is journaled
  through `SetFacet`; the two schema ids are in Knot, Turnstone and
  eidetic-core. (A) keep all three verbatim: they are data identifiers, not
  paths. (B) rename to `portolan.*` with a reader that accepts both: user data
  justifies the alias under DOC_POLICY §3, but it is new code. (C) rename with no
  alias: loses stored class memberships.
- **F10. First versions.** (A) all four at 0.1.0; READMEs state the lineage.
  (B) portolan at 0.4.0, continuing chartulary's line, the other three at 0.1.0.
- **F11. The G0 to G3 doc file names.** (A) keep the dated file names; update
  their text and DOC_README descriptions: no links to repair. (B) rename them
  `portolan_g0` to `portolan_g3` (G3 to quadstore's name), repairing links
  under DOC_POLICY §5.
- **F12. When to publish.** (A) right after P1 lands on main, before any
  consumer moves: claims the names early, and consumers can take registry
  versions. (B) after P3, once the consumer moves have shaken out any API fix:
  the names stay unclaimed meanwhile.

## Progress

- 2026-11-12: plan written from measurements at the origin/main commits named
  above. No code changed.
