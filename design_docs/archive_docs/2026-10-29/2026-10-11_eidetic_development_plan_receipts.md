# Eidetic model-free retrieval receipts

## Prior planning basis, 2026-10-26

Moved from the active P4a plan under DOC_POLICY §8; these remain historical source and software checks.

Fetched upstream before planning and fast-forwarded clean consumer checkouts.
The inspected main snapshots are Eidetic
`d964d664900ca8cb157e1fd38059ae46eb432916`, Mere
`7740abc107e15abb54dfa478bcca6a16c4d47173`, and Turnstone
`309ea73f734c618a987393b2c6b10fd2ff4951b7`. These identify inspected source,
not a newly tested executable. Refresh them again before implementation.

- Lexical recall moved from Mere to this repository at `098e8a1`, with source
  qualification at `354ab71` and receiving recorded at `d964d66`.
  `crates/eidetic-search/src/{document,index,candidates,tokenize,fusion}.rs`
  already supplies weighted BM25, exact-address lookup, prefix candidates,
  reports and optional ranking fusion. Default production lexical dependencies
  do not require an embedding runtime; ESP remains an optional/example/test edge.
- Both rewritten `codex/cross-source-answers-20261022` tips are ancestors of
  their current consumer mains. Preserve the integrated history. Do not merge
  old pre-rewrite branches or cherry-pick this work a second time.
  `turnstone/docs/receipts/eidetic_main_integration_20261026/README.md` records
  331 focused passing software checks and source-coherence checks. That receipt
  explicitly does not qualify a new model-free product path.
Current consumers deliberately name different Eidetic revisions: Mere's core
pin is `af17fe7`, Turnstone's core pin is `f599ffe`, and both search pins are
`354ab71`. Reconcile the actual resolved type-bearing dependency graph when
receiving the implementation. Do not blanket-relock unrelated suppliers or
assume these manifest differences alone imply duplicated runtime types.


## M1 implementation, 2026-10-29

Provider validation passed; consumer receiving remains in progress. No native product or offline restart claim.

Provider base: `f42aab7df545d957dcb4b38e533eb925e8dab8a7`, freshly fetched before publication. All compiler runs use the existing `machine-heavy.lock`, 3 GiB MemoryMax, zero swap, 150% CPU quota, one Cargo job and one test thread, with a 600-second timeout. Guard commands and log checksums are retained in [model-free-m1](model-free-m1/).

- Initial red gate failed at the missing `eidetic_search::recall` import.
- The emission-time scope-change regression failed before the final recheck was added.
- Final shared tests: 51 passed, 3 measurement tests ignored.
- No-default-features provider check passed. The normal/build feature tree has no ESP or GPU runtime. Test-only ESP is not a production dependency.
- Source review found and fixed premature character-buffer allocation and the consumer's retained duplicate plaintext snapshot. An embedding-only descriptor type error was also corrected before receiving checks.
- Exact Unicode JSON and passage ID are pinned in the regression test. The core passage and hash serialization files are byte-identical to the pre-extraction provider base; generated-answer wire schemas are untouched.

| Gate | Exit | Log SHA-256 |
|---|---:|---|
| `eidetic-m1-red-01` | 101 | `a6b9507e5a6f0f4358de08011c60df7ee7a02a4a0a957a03354799d945928257` |
| `eidetic-m1-recheck-red-01` | 101 | `fd329f45b49ae6d0ae5a8793d64b1474cd4fa86faf5b266b80a0772b70cc1c8a` |
| `eidetic-m1-final-01` | 0 | `397879b8082571d8b0f9114517b4fdb95cd1a7ba340c229ea634dd97ebd2476b` |
| `eidetic-m1-nodefault-01` | 0 | `394691e9e380590aed7ce310304a9ee738567363b3e60d1bf0277054727d93d6` |
