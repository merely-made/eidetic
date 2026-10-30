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

M1 provider and receiving checks passed. Earlier resource-limited and superseded attempts are retained below. No native product or offline restart claim.

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


### Shared provider and Distillery receiving

Published provider `01ee97b0b95d679430986dffd54c64ce9b3b5ea3`; Mere adapter and coherent Eidetic-family pins published as `f631d1ec8909b978f1a31bce9e30f3e5bf65be78`.

- Wasm no-default-features check passed for `wasm32-unknown-unknown`. This is compilation, not headed browser qualification.
- Distillery `inference-model`: 15 recall tests passed.
- Distillery `inference-embedding`: 19 lexical/embedding recall tests passed. This includes reopening, scope-before-fusion, changing heads, private cache failure, context budgets and source-identity-independent ties.
- The consumer's frozen snapshot is consumed once, avoiding a retained extra plaintext copy. Legacy generation explicitly expands empty scope and uses its established single head observation plus later evidence assessment. It does not substitute for the new search API's admission revision checks.
- Mere's lock update changes only the Eidetic family. Turnstone's initial update incidentally advanced Murm and NetRender; precise updates restored both to their original locked revisions. No other package identities changed outside Mere/Eidetic.
- The resolved Turnstone search normal/build feature tree also excludes ESP and GPU runtime dependencies. Other application features retain their own inference and rendering dependencies.
- `receiving-graph.json` records the resolved singleton core/search/storage providers and lock hashes. Turnstone receiving is now published; final check and limits are recorded below.

Full commands, outcomes and checksums are in the named JSON files beside their logs.


### Turnstone test-build resource boundary

`turnstone-m1-readers-01` exited 143 before tests ran. Systemd records
`Result=oom-kill`, `MemoryPeak=3221225472`, `OOMPolicy=stop`. This was the
3 GiB cgroup ceiling, not the 600-second timeout. The failed log, wrapper
receipt, scope properties and journal are retained. No assertion result is
claimed, no process outside the scope was killed, and the guard was not raised.
The separate final receiving check passed, as recorded below.


### Exact lock preservation

The deeper dependency-edge audit found Cargo had also reselected existing
Windows bindings and, in Turnstone, thiserror bindings without changing their
package identities. The final locks were rebuilt from their original locked
text with only the intended Eidetic/Mere source revisions replaced. Full
normalized lock equality now passes (`model-free-m1-lock-audit.json`), and
Cargo accepts both with `--locked`.

Mere's lock-only correction is `e964db71a`; the final embedding-enabled recall
rerun passes all 19 tests. Turnstone keeps the code revision `f631d1ec8` because
the subsequent supplier correction changes only its workspace lockfile, which
a consumer does not inherit. The initial Turnstone application-test attempt and
first check used the intermediate receiving graph. That check was deliberately
stopped when the incidental edges were found; `turnstone-m1-check-02` checks the
final preserved graph. No test pass is inferred from either interrupted run.


### Receiving check before the concurrent upstream advance

Turnstone candidate `1d4f5d3db72642055c9a00017c197a7a0bd00c6e` contained the shared code before its push was rejected. Its
`cargo check --locked -p turnstone --lib` passes under the unchanged guard;
maximum process RSS was 1,572,696 KiB, with zero process swaps. This is compile
qualification, not a completed application test or native restart run.
Mere main includes the lock-only correction at `e964db71abae92fac98669f5d1174b954b706034`.

M1 is complete at its stated boundary: a model-free test caller retrieves exact
passages, and existing inference uses the same lexical implementation. The final
provider has 51 passing tests and three ignored measurement harnesses; Distillery
has 19 passing lexical/embedding recall tests, including the 15 lexical cases.
Native and Wasm provider checks pass. M2–M5 remain open, including a real capture,
selected clip and note through the product UI and fresh offline restart.

Review also verified that the old search source files are unchanged from
`354ab71` and that the other Eidetic crate code is unchanged from the prior
consumer pin `e7eef1e`; only the shared recall module, export and tests were
added. Core passage bytes and IDs remain under their original serialization.


### Concurrent upstream reconciliation

Turnstone advanced to `571c2f3c83d4ec93710dc60b65cef75254e46487`
(source-size enforcement and demand-driven service composition) before the
receiving push. The non-fast-forward push was rejected; the unpublished commit
was rebased, preserving the new optional services, Armillary/Woodshed pins and
source split. Only the intended Eidetic/Mere revisions are replaced in the new
upstream lock. The earlier Turnstone receipts are historical to their earlier
source and feature composition, not qualification of this rebase.

The rebased default and inference-feature dependency boundaries pass, as does
the 600-line owned Rust-source limit. Saved-source and research tests pass on the
lighter default build under the unchanged guard.


The cold rebased run `turnstone-m1-readers-02` exited 124 at its 600-second
timeout after fetching the newly pinned Woodshed source and rebuilding changed
feature sets. The warm retry `turnstone-m1-readers-03` passed all 21 selected
saved-source cases, with maximum process RSS 2,577,384 KiB and zero process swaps.
`turnstone-m1-research-01` passed all nine selected research cases. Neither the
3 GiB limit nor the CPU, swap, job-count or timeout settings was raised.
These are real Turnstone library tests on the rebased source; no headed/native
producer or fresh process restart claim is inferred from them.


### Final publication and qualification

Turnstone receiving is published as `5c06e1ad7223c090ce327e36e492e9c5ccc30f03`,
rebased on `571c2f3c`. The final optional `inference` library check also passes
(`turnstone-m1-inference-check-01`); it is compilation, not live generation.
The final graph audit preserves every upstream dependency selection outside the
intended source revision substitutions, including the newly received Armillary
and Woodshed pins. The final source-size, default-service and inference-service
boundary reports are retained alongside the gate receipts.

Final scoped results: 51 shared tests passed (three measurement harnesses ignored),
19 Distillery lexical/embedding recall tests passed, 21 Turnstone saved-source
tests passed, nine Turnstone research tests passed, and provider native/Wasm plus
Turnstone optional-inference compilation passed. The shared normal/build feature
tree activates no model or GPU runtime. All compiler gates kept the same resource
limits. Broader application suites, native producer UI, model-free restart and
M2–M5 product completion remain unclaimed.
