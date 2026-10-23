# Eidetic

Reusable durable memory, graph history, replication, artifact distribution and
participant gates. Applications compose these libraries without adopting Mere's
application shell or surrendering their domain authority.

The implementation history was promoted from Mere on 2026-10-19. Crates retain
their names, formats, APIs, licenses and selectable features. Canonical migration
and its historical receipts:
`mere/design_docs/mere_docs/implementation_strategy/2026-10-19_family_promotion_plan.md`.

| Crate | Responsibility | Boundary |
|---|---|---|
| [muniment](crates/muniment) | Byte backends, immutable blobs, mutable slots, journals and atomic storage operations | Callers choose the backend; transaction support is a backend capability |
| [chartulary](crates/chartulary) | Generic resource graph, attributed edits/assertions and owner-scoped Stemma encounters | Domain meaning and destination admission stay with the caller |
| [eidetic](crates/eidetic-core) | Typed retained content, manifests, schemas, intake, source evidence, browsing corpus and model-work artifacts | Stores and fetchers are injected; possession supplies no disclosure or edit authority |
| [hagiograph](crates/hagiograph) | Standing marks, significant events and a product-driven deep-time seam | Products define significance and run their own simulation |
| [stickleback](crates/stickleback) | Replicated-operation storage, policy-before-insert, retention/epoch mechanics, group state and bounded drop carriage | Domains supply addressing, authorization and deterministic materialization |
| [luggage](crates/luggage) | Signed application-release identity and optional native update/install mechanics | The release-identity core is usable without the native updater |
| [mere-proofs](crates/proofs) | Typed commitments, digests and content references | Operation identity and application commitments remain distinct |
| [servitor](crates/servitor) | Scoped participant petitions, graph admission, watches and recorded run reduction | Hosts execute work, supply time and reconcile outcomes; this is not an execution supervisor |
| [wing-impresa](crates/wing-impresa) | Validated association records for anything pointable, and their lapses | Products store the records in their own accepted history and derive readings from them |
| [wing-decisions](crates/wing-decisions) | Decisions as linked structure (DOC_POLICY core §11): manifest, register, lookup, budget, links across repositories | Repositories keep their own cases and doctrine; both names wait on a naming round, so neither publishes |

Fleece extraction/selected-package adapters, lexical recall, Pandect sessions,
Inker document routing and application adapters remain in Mere. ESP owns model
execution and embeddings; Dramatis owns identity/key vocabulary; Murm owns
transport; Gemot owns community authority. The independent source home does not
require those integrations to move with it. The current boundaries, verified
source cuts and remaining qualification gates live in the
[development plan](design_docs/eidetic_docs/implementation_strategy/2026-10-11_eidetic_development_plan.md#post-extraction-stack-review-2026-10-20).

Run `cargo test --workspace` for default library coverage. CI also checks the
workspace with default features disabled. Network, browser storage, native
backend combinations, GPU/model and host acceptance retain their separate gates.
A library pass does not qualify every backend or an installed personal custodian.
No registry release is implied by source promotion.
