# Eidetic documentation

Stickleback's network-session extraction (2026-11-03) is tracked by
`murm/design_docs/2026-10-28_transport_consolidation_plan.md`, T7. The core keeps
validated storage, causal processing, group encryption and admission; Murm
receives live joining/sync and transport address books.

Preserve host-owned authority, existing formats and independent crate feature boundaries. Read [documentation policy](DOC_POLICY.md) before editing. The maintainer-owned `PROJECT_DESCRIPTION.md` is absent from this checkout.

`eidetic-search` keeps only the `GoalEmbedder` trait (2026-11-06, Mere `design_docs/mere_docs/implementation_strategy/2026-11-06_cycle_breaks_plan.md` P4, D2): the ESP adapter, the MiniLM calibration test and the `eidetic-recall` example live in ESP (`distillery/crates/esp`), and Eidetic's manifests and lock name no `esp`.

Supplier receiving audits compare complete locked dependency edges as well as package identities; preserve unrelated selections when updating a source revision.

Registry release work (2026-11-03) is tracked in
`mere/design_docs/mere_docs/implementation_strategy/2026-10-28_workspace_hygiene_plan.md`:
Muniment's atomic-file API, the carried Fjall and LSM-tree forks required by its
optional backend, and aligned Chartulary and Eidetic releases for custody and
Pandect consumers. Existing persistence formats and owner policies remain in
their owning plans.

Canonical migration and current receipts: `mere/design_docs/mere_docs/implementation_strategy/2026-10-28_family_promotion_plan.md`. The plan it was cut from, `2026-10-19_family_promotion_plan.md`, is archived at `mere/design_docs/archive_docs/2026-10-28/2026-10-19_family_promotion_plan.md`.

## Family documents

- [2026-11-01_memory_gates_plan](eidetic_docs/implementation_strategy/2026-11-01_memory_gates_plan.md) — plan, a draft awaiting review; cut from the Eidetic development plan (D1). Carries every open item: the P0 capability ledger and owner inventory, the encounter, admission, capture and sealed-custody gates left open after the first browser encounter slice (P1-P3, with the HTTP pooled-cancellation race gate), P4's remainder (goal/query consumer over Fleece and ESP, and tag suggestions), which is next (D2), headed Chrome and Firefox qualification (P5), and storage, custody, retention and exchange (P6, with the `Backend::transact` checkpoint). The run outputs it cites left the repository for `Code/testing/eidetic/` (D3).
- [2026-10-23_responsa_plan](eidetic_docs/implementation_strategy/2026-10-23_responsa_plan.md) — active; fading of doctrine statements and the answer-key rerun on toolchain bumps, both owed by isocosm's decision-links plan; nothing built.
- [2026-10-30_line_ceiling_plan](eidetic_docs/implementation_strategy/2026-10-30_line_ceiling_plan.md) — landed 2026-11-02; the shared 600-line binary, standalone tagged tool installs on Windows and Linux, and adoption in isocosm and eidetic. Eidetic's adoption baseline lists 20 owned exceptions; H6 retires fifteen while imported patches remain excluded by path; commit and full-tree rejection controls pass. Stack-wide adoption remains in Mere's house-style plan.
- [2026-05-09_eidetic_design_pass](eidetic_docs/research/2026-05-09_eidetic_design_pass.md) — research synthesis (2026-05-09) proposing the four-layer private-memory stack Eidetic grew into; largely built, with former crate names and donor paths historical.
- [2026-07-08_codicil_founding_proposal](eidetic_docs/research/2026-07-08_codicil_founding_proposal.md) — founding proposal for codicil, the append-only replayable log over muniment persistence.
- [2026-07-08_muniment_founding_proposal](eidetic_docs/research/2026-07-08_muniment_founding_proposal.md) — founding proposal for muniment, the portable persistence store and the access patterns over it.
- [2026-07-08_scholia_founding_proposal](eidetic_docs/research/2026-07-08_scholia_founding_proposal.md) — founding proposal for scholia, the RDF projection over a chartulary graph (export half built).
- [2026-09-04_eidetic_review_brief](eidetic_docs/research/2026-09-04_eidetic_review_brief.md) — review brief (2026-09-04): what the family holds, what search costs and what nothing uses; findings, with questions for Mark.
- [2026-09-07_lighter_recall_and_standards_ledger_brief](eidetic_docs/research/2026-09-07_lighter_recall_and_standards_ledger_brief.md) — brainstorm brief (2026-09-07) on a lighter recall approach than Tantivy and a standards-to-features ledger.
- [2026-11-05_browser_storage_posture_brief](eidetic_docs/research/2026-11-05_browser_storage_posture_brief.md) — brief, unreviewed (2026-11-05): who writes the browser mere today (Graphshell extension pages under its existing Web Lock), the read-only follower handle, and the redb-over-OPFS adoption question; recommends IndexedDB now and OPFS only behind a named gate, with seven decisions for Mark.
- [2026-07-08_chartulary_g0](eidetic_docs/technical_architecture/2026-07-08_chartulary_g0.md) — G0 of the generic graph substrate: the generic graph, capability traits, default payloads and two-ring taxonomy.
- [2026-07-08_chartulary_g1](eidetic_docs/technical_architecture/2026-07-08_chartulary_g1.md) — G1: the edit spine, an event-sourced graph where mutations are logged edits and replay rebuilds the graph.
- [2026-07-08_chartulary_g2](eidetic_docs/technical_architecture/2026-07-08_chartulary_g2.md) — G2: fork and lineage, self-contained graph forks and per-node derivation records.

## Evidence archive

- [Eidetic development plan (archived 2026-11-01)](archive_docs/2026-11-01/2026-10-11_eidetic_development_plan.md) — superseded; cut into the memory gates plan and archived whole. P4a (M1-M5) landed for the recorded Linux cohort: the W intact plus W3 missing-source collection passed all 12 structured operations, 62 frames were reviewed, the 1,403/0/21 host suite is software evidence, and model semantics remain 8/15. The M1-M5 run outputs and the frozen M5 harness live under `Code/testing/eidetic/`.
- [Eidetic development plan receipts](archive_docs/receipts/2026-10-11_eidetic_development_plan_receipts.md) — consolidated M1-M5 receipts for the recorded Linux source cohort (Turnstone `554f323f`, Mere `845fe1c4`, Genet `b7c0bef`), including the M5 final W3 receipt and its independent receipt verifier and audit, and the supplier, helper and failed-attempt records. The run outputs they cite are at `Code/testing/eidetic/` (path cites, not links). Non-Linux, hardware, personal-custody, power-loss and general-accessibility qualification remain open.

Distillery receiving (2026-11-03): ESP keeps its crate name and now comes from
`distillery.git`, after the history-preserving repository move. The canonical
cutover plan and run receipts are in
`mere/design_docs/mere_docs/implementation_strategy/2026-10-31_distillery_extraction_plan.md`.


State witness and framing receiving (2026-11-07): `state-witness` and `mere-framing`
are workspace members at `crates/state-witness` and `crates/framing`, copied byte for
byte from Mere `0adc6b1dc` (`crates/system/`), so Seiche's replay and Isocosm take them
from `eidetic.git` without a Mere edge. Package names, versions and the separate
framing crate are kept. The ruling and cutover are Mere's:
`mere/design_docs/mere_docs/implementation_strategy/2026-11-07_engine_split_plan.md`, D12.

- [Source splits, H6 and H8](eidetic_docs/implementation_strategy/2026-11-03_source_splits_plan.md): fifteen earlier and five additional originals landed; zero owned exceptions remain. Mere owns the aggregate; source notices, public paths, authority and existing acceptance boundaries stay intact.
