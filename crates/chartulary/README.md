# chartulary (transitional shim)

`chartulary` split on 2026-11-12. This crate only re-exports the four crates it
became, under the old paths, so existing consumers compile while each moves:

| Old path | New crate |
|---|---|
| `chartulary::<core item or module>` (graph, spine, commit, facet, canonical, rdf, …) | [portolan](../portolan) |
| `chartulary::stemma` | [hodograph](../hodograph) |
| `chartulary::quads` | [quadstore](../quadstore) |
| `chartulary::{field, field_ast, coupling, edge_path}` | [fieldbook](../fieldbook) |

It is git-only and never published. It is removed once no manifest names it
(plan P4: `design_docs/eidetic_docs/implementation_strategy/2026-11-12_portolan_plan.md`).
The crates.io name is to be released back to the pool once nothing published
depends on it (P6).

License: MPL-2.0.
