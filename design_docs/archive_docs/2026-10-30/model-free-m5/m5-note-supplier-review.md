# M5 note supplier review

Verdict: **pass, source review only**. No blocking findings in the scoped supplier changes. This does not approve the unfinished Turnstone adapter or qualify the product workflow.

Reviewed Woodshed base `b790b3c1f9c4a08a599e5ad746c39134ee22810a` plus four modified files. `git diff --unified=8` SHA-256 is `8506ffdb7f6ba59db27e6b0810efcc8b45dd848492bf3c353d6215d72c87a235`, matching `m5-note-supplier-review.diff`.

Exact reviewed file SHA-256:

- `ports/redshank/surfaces/src/dock.rs`: `2bdd2bedc570b39cdcc8037a59b3e7cc74e3eb4f58eeda7a524d282197d80b98`
- `ports/redshank/surfaces/src/lib.rs`: `c4b18271358ff17e0f3771655c98473748372b7b31f384cf1d0bda76416264d1`
- `ports/redshank/surfaces/src/tabs/listen.rs`: `871570b16f1859616c729d3c29e6d8d5d3d2fc437a5077ec042c1d87c68fa698`
- `ports/redshank/surfaces/src/tabs/rows.rs`: `48fb57b0d6676e07a6cf847b64138c7d099ce1ad8e4eb45cc2df35111a8a6977`

Source findings: `Full` is the default and retains the prior Edit, privacy and Delete actions. `PrivacyOnly` suppresses Edit and Delete in the shared row while leaving privacy and the existing editor intact. The optional capture capability defaults to `None`, preserving transport-derived behavior; either explicit value is still gated by a current item. The existing text-note button is reused in the dock, capture surface and rail, with no synthetic transport state or duplicate editor. Supplier constructors found in the tree use defaults or struct update syntax. Downstream reception and constructor migration remain the host's responsibility.

Evidence boundaries: the two intended red tests are reported in `m5-note-controls-report.md`; this reviewer did not rerun them. The nested `redshank-surfaces --lib` gate was queued at review time and its result is pending. No build, runtime, headed UI, accessibility, persistence or product-use verification was performed by this reviewer. The tests in the diff exercise privacy-only text-row behavior and the explicit capture states, while existing tests cover default row actions and capture behavior; a successful gate is still required to establish compilation and execution on these exact files.
