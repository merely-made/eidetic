# Eidetic

Durable content, graph history, replication, artifact distribution and participant gates.

This repository preserves the implementation history promoted from Mere on 2026-10-19. Crates retain their names, APIs, licenses and selectable features.

Canonical migration: `mere/design_docs/mere_docs/implementation_strategy/2026-10-19_family_promotion_plan.md`.

| Crate | Source |
|---|---|
| `muniment` | [crates/muniment](crates/muniment) |
| `chartulary` | [crates/chartulary](crates/chartulary) |
| `eidetic` | [crates/eidetic-core](crates/eidetic-core) |
| `hagiograph` | [crates/hagiograph](crates/hagiograph) |
| `stickleback` | [crates/stickleback](crates/stickleback) |
| `luggage` | [crates/luggage](crates/luggage) |
| `mere-proofs` | [crates/proofs](crates/proofs) |
| `servitor` | [crates/servitor](crates/servitor) |

Run `cargo test --workspace` for default library coverage. Optional network, GPU, external model and host acceptance have separate gates. No registry release is implied by this source promotion.
