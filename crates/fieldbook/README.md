# fieldbook

Graph fields as portable data. Fields are the third graph primitive beside
nodes and edges.

| Module | Contents |
|---|---|
| `field` | `Field`, `FieldId`, `CouplingId`, `FieldDefinition`, `FieldExtent`, `FieldLifecycle`: identity, extent and lifecycle |
| `field_ast` | `ScalarField`, `VectorField`, `Falloff`: the scalar/vector field AST |
| `coupling` | `Coupling`, `CouplingResponse`, `NodeSelector`, `COUPLING_VOCAB`: how a node responds to a field |
| `edge_path` | `EdgePath`, `EdgePathRule`: how an edge is drawn |

Definitions only. Evaluation is not here; the numen crate in Seiche evaluates
them. `FieldId::new` and `CouplingId::new` need an OS randomness source and are
native-only; wasm hosts build ids with `from_uuid`.

Dependencies: `serde`, `uuid` (`serde`; `v4` off wasm32, `js` on
wasm32-unknown-unknown).

## Where this came from

These modules moved from numen into the `chartulary` crate on 2026-11-09 (Mere
engine-split D49), and from there into this crate on 2026-11-12, unchanged. The
split is planned in Eidetic's
`design_docs/eidetic_docs/implementation_strategy/2026-11-12_portolan_plan.md`.

A field book is a surveyor's record of what was measured in the field.

License: MPL-2.0.
