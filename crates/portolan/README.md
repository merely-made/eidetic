# portolan

The generic content-addressed container graph. A `Graph<N, E>`
where nodes are content-addressed containers and edges are typed relations, over
one shared, app-agnostic model. The substrate that binds
[muniment](https://github.com/merely-made/mere)'s blobs and
muniment's append-only journal into a graph.

Fully generic: a node needs one capability, `Identified`, to live in the graph.
Everything else is an opt-in trait that unlocks a feature.

```rust
use portolan::{Container, Graph, Recognized, Relation, RelationClass};

let mut g: Graph<Container, Relation> = Graph::new();
let a = g.insert(Container::new("a").with_title("Article A").with_tag("research"));
let b = g.insert(Container::new("b").with_title("Article B"));
g.connect(a, b, Relation::new(RelationClass::recognized(Recognized::Cites)));

// query by identity, by neighbor, by relation class, by tag
assert_eq!(g.get(&"a".to_string()).unwrap().title, Some("Article A".into()));
let cites = RelationClass::recognized(Recognized::Cites);
assert_eq!(g.out_edges_of_class(a, &cites).count(), 1);
assert_eq!(g.nodes_tagged("research").count(), 1);
```

Relations come in two rings: a shared **semantic** ring (recognized core plus open
predicate IRIs) that projects to RDF, and **app-private** families that do not.
The provided `Container` and `Relation` payloads implement every capability trait;
an app that needs more implements the traits on its own types.

## Modules

| Module | Contents |
|---|---|
| `caps` | `Identified`, `Addressed`, `ContentBearing`, `GraphBearing`, `Labeled`, `Classified`, `Predicated`, `Address` |
| `graph` | `Graph<N, E>`, `NodeKey`, `EdgeKey` |
| `container` | `Container`, `ContainerAddress`, `Relation` |
| `taxonomy` | `RelationClass`, `Recognized`, `Semantic`, `REL_NS` |
| `rdf` | RDF export of the semantic ring: `to_quads`, `to_jsonld`, `to_nquads`, `Quad`, `Term` |
| `edit` | `GraphEdit`, `EdgeId`, `WriterId`, `DerivationKind`, `DerivationRecord` |
| `spine` | `GraphLog`: graph edits as a muniment journal, with snapshots for checkpoint-plus-tail loading |
| `commit` | `Batch`, `BatchId`, `Author`, `EditSpec`, `Committed`, `CommitError` |
| `assertion` | `AssertionRecord`, `AssertionSpan`, `AssertionWithdrawal`, `AssertionBasis`, `AssertionMethod`: retained, source-attributed relations keyed by `EdgeId` |
| `facet` | `NodeFacets`, `FacetStore`, `FacetId`, `FacetValidator`, `AcceptAll`, `FacetError`, `ExpiringFacet`. Runtime typed node metadata, validated through an injected seam; revision-indexed shelf life stays a read predicate and never mutates stored state |
| `content_class` | `ContentClass`, `ClassId`, `ClassRegistry`, `ClassMembership`, `CLASS_FACET`, `ClassError` |
| `canonical` | `canonical_url`, `resource_id`, `resource_id_from_canonical_iri`: canonical resource addresses and their UUIDv5 ids |
| `nested` | Slot-name helpers: `log_slot`, `snap_slot`, `archived_log_slot`, `archived_snap_slot` |

`muniment::LogId` and `muniment::Provenance` are re-exported so a consumer can
fork a log and inspect provenance through the same storage floor.

`GraphLog::assert_relation` creates an independently addressable assertion,
with a source asserter separate from the admitting batch author. Explicit
confirmation and withdrawal use its `EdgeId` through the same atomic batch
seam. Withdrawal retains the assertion and its explanation while removing it
from the held graph; a later assertion receives a new id. The caller supplies
record times, and `None` remains unknown. Replay never samples a clock.

Legacy connections recover their known batch author and unknown times. Old
JSON checkpoints without a ledger reconstruct it from the full retained
journal, so that migration requires the append-only prefix. New checkpoints
materialize the held graph from the ledger and retain withdrawn ids. The
graph-only RDF functions still export held semantic triples; assertion
reifiers and explicit retained-history RDF export remain a separate adapter.

Dependencies: `petgraph` (with `serde-1`), `muniment` (default features off, for
the blake3 `Hash` and append-only journal), `serde`, `serde_json`, and `uuid`
(`v5`, for `canonical`).

## Where this came from

Until 2026-11-12 this code was the core of the `chartulary` crate (aka
**chart**; a chartulary is the register a house kept its charters and muniments
in). That crate split four ways: this core became `portolan`; its lineage layer
(`chartulary::stemma`) became [`hodograph`](../hodograph); its quad index
(`chartulary::quads`) became [`quadstore`](../quadstore); and its field
definitions became [`fieldbook`](../fieldbook). Item names and behaviour are
unchanged; only the paths moved. The split is planned in Eidetic's
`design_docs/eidetic_docs/implementation_strategy/2026-11-12_portolan_plan.md`.
The RDF projection was folded into chartulary from the standalone `scholia`
crate; it projects only the semantic relation ring, leaving app-private families
out of the export.

Build phases and their done conditions live in the canonical plan, mere's
`design_docs/mere_docs/technical_architecture/2026-07-08_generic_graph_substrate_plan.md`,
and are not restated here: the module table above is what the crate actually
ships, and a phase ladder copied into a README drifts from it.
See Eidetic's [`design_docs/`](https://github.com/merely-made/eidetic/blob/main/design_docs).

License: MPL-2.0 (see LICENSE).
