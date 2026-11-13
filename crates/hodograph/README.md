# hodograph

An owner-scoped lineage model: the descent of entries through branching visits.
Which things were engaged, in what order, branching where history diverged,
shared across the cursors that walked them.

- `EntryRecord`: the deduplicated content identity; many visits can resolve to
  one entry.
- `VisitRecord`: a concrete, persisted occurrence with a parent and zero or more
  children; the branching unit.
- `OwnerRecord`: a cursor-bearing actor (a view, a session) that tracks an
  origin and a current visit and owns a set of visits.
- `EdgeView`: a derived projection over visit parentage.

Visits own the tree; edges are projected from visits, never stored separately.
The model is generic over the entry key, the entry payload, the owner identity
and the per-visit context. `Stemma` is the model and `StemmaSnapshot` its
serde/rkyv snapshot. `TransitionKind` names how an owner moved to a visit, in
navigation vocabulary; generalizing it is an open decision.

Dependencies: `serde`, `rkyv` (`std`), `slotmap`.

## Where this came from

Until 2026-11-12 this was the `stemma` module of the `chartulary` crate
(`chartulary::stemma`), and before 2026-07-12 a standalone crate of that name
(the crates.io name `stemma` belongs to an unrelated project). It moved here
unchanged: item names, including `Stemma` and `StemmaSnapshot`, and serialized
forms are as they were. The graph core it projects over is
[`portolan`](../portolan). The split is planned in Eidetic's
`design_docs/eidetic_docs/implementation_strategy/2026-11-12_portolan_plan.md`.

The Entry / Visit / Owner data model is adapted from Atlas Engineer's
[`history-tree`](https://github.com/atlas-engineer/history-tree) (Common Lisp,
BSD-3-Clause), written for the Nyxt browser. This is an independent Rust
reimplementation against the same abstract data model; no `history-tree` source
was translated.

A hodograph is the curve traced by a moving body's velocity vectors, drawn from
one origin: a record of how something moved.

License: MPL-2.0.
