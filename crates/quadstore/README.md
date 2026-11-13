# quadstore

A dictionary and a quad index over dense ids, generic over the term type.

Every term a quad uses is interned once in a `Dictionary` and named by a
`TermId` (a `NonZeroU32`). The quads are held as id tuples in a `QuadIndex`
sorted six ways (SPO, POS, OSP and their graph-first twins), so any pattern that
binds some of subject, predicate, object and graph reads one contiguous range.

- Dense ids per rebuild: nothing of a `TermId` is persisted.
- `QuadIndex::apply` appends asserted rows, flips withdrawn ones and tombstones
  removed ones in place; `QuadIndex::build` stays the fallback and the oracle.
- One index, two views: `View::Live` reads held rows, `View::History` reads all.
- `QuadMeta` carries `held`, and an optional asserter and statement.

The crate never learns what a term is. Mere's engine interns `oxrdf` terms and
implements its SPARQL evaluator's dataset seam over this index;
`portolan::rdf::Term` works as well. `examples/quads_apply.rs` times
maintenance against rebuilds (`cargo run --release -p quadstore --example
quads_apply`).

No dependencies.

## Where this came from

Until 2026-11-12 this was the `quads` module of the `chartulary` crate
(`chartulary::quads`). It moved here unchanged. The design is ruled in Mere's
`design_docs/mere_docs/technical_architecture/2026-11-09_indexed_dataset_brief.md`
and `design_docs/mere_docs/implementation_strategy/2026-11-09_quad_index_plan.md`;
the split in Eidetic's
`design_docs/eidetic_docs/implementation_strategy/2026-11-12_portolan_plan.md`.

License: MPL-2.0.
