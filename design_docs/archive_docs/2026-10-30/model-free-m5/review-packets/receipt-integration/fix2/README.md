# Draft M5 receipt integration

This is a review packet, not an acceptance verifier. The existing collector and fault run remain read-only. `assemble.py` requires a new output directory separate from both inputs. It validates both existing ledgers before copying, binds the fault snapshot to the intact ledger and selected profile file, checks the intact build/source receipt links and frozen executable hashes, and copies ledger-bound files plus the selected owner file. It copies no owner stores or executable binaries. `verify.py` checks the copied ledgers, process and namespace evidence, reviewed semantic functions and the complete missing-case `verify_bound_case`, then always raises `DRAFT ONLY`.

Validation code from outside this directory is copied only if its exact SHA-256 matches `assemble.TOOLS`. The verifier executes the pinned `verify_receipt.py` and `verify_missing.py` bytes directly; it compiles only the `verify_bound_case` AST from the pinned `run_missing.py`, avoiding that launcher's live imports. These pins require independent review and a new pin after any upstream change. The two integration scripts themselves must also be frozen and hash-reviewed as part of the final packet.

A human review JSON must contain `reviewer`, a `reviewed_frames` array of receipt-relative paths, and `outcomes` mapping every required frame to `pass`. Required coverage includes all operation frames, cold replay, private/shareable note controls, each negative, and the three missing-source frames. A PNG and a nonempty review list do not themselves demonstrate a correct outcome.

The verifier requires the comparison observation and frames to match both exact owner-opened versions and to account for their entire scalar text. It checks the note's citation provenance and rendered quoted passage, selection and note search identities, and complete filtered Sum, group, pair, count and source contributors. No-match is tied to the current owner-backed scope and exact absent-property query. The over-limit check requires the actual worker-refusal status with a bound request, query, one-record setting, current scope and the exact `Structured record limit exceeded` error. That observer change is in the product source and needs its own focused build and review before a matching native collection can supply this evidence.

Cold Selection and Note evidence additionally requires the actual retained reader target and rendered body, matched to the accepted search hit's observed selector and the independently inspected admission and version. Search observations and post-open reader observations and frames are separate. This requires the pending product observer and scenario changes; the existing search-hit-only reopen files are insufficient. The A1 citation binds the complete 57-scalar fixture passage, while the rendered reader check also confirms its distinctive shorter Unicode substring.

The inspected historical Sum snapshot is an independently checked complete inventory of all 14 scoped graph records, their definitions and authenticated sources. Each pre-withdrawal operation must carry the same session, selected-source scope, graph and complete input inventory; the verifier applies its explicit source, tag, entity, property, revision, status, unit and half-open time filters before checking rows. Separate historical and current withdrawal projections must differ by exactly the one graph-marked withdrawn record. Scope revision tokens may change as saved results are written; inventory and graph identities are checked instead.

For selected-source evidence, the inspected source is bound to the pre-inspection profile SHA-256 inventory and copied unchanged into the receipt. The matching source-bound inspector independently checks the private backing bytes and BLAKE3 anchor, and the verifier checks the exported anchor against the inspector output and the source's byte and scalar selections. This proof relies on the reviewed inspector and matching build provenance; it does not independently reimplement BLAKE3 in Python. `resident_model_loads` remains unknown. A successful native intact run, a successful separately reviewed fault run and a human frame review have not been supplied. Keep the final guard until real evidence and independent review.

Frozen collector scripts and scenarios must match the collector input hashes and ledger in both assembly and standalone verification. The executable binaries are intentionally not copied: the receipt records their hashes checked against the original frozen binaries at assembly, while the standalone verifier rechecks those recorded hashes against the copied build/provenance documents. This is an assembly-time observation, not independent access to the original executable bytes during later verification.

Usage after a real reviewed intact and fault run:

```sh
python3 assemble.py --intact /path/to/intact --fault /path/to/fault --review /path/to/review.json --output /path/to/new-receipt
python3 verify.py /path/to/new-receipt
```

Both commands refuse absent or changed evidence. A failed assembly may leave a partial new receipt directory; it does not alter the source runs. Remove or preserve that output explicitly before trying another fresh path.

## Fault-driver status

The approved fault-ledger fix reconciles the acceptance helper API and writes its file ledger after owned-scope cleanup and primary integrity checks. The fault runner remains disabled pending the actual run and matching source/build cohort. Its focused review is `../m5-missing-draft/fault-ledger-fix1-review.md`; the pinned runner hash is recorded in `verify.py`. No completed native fault sample or headed approval is claimed.
