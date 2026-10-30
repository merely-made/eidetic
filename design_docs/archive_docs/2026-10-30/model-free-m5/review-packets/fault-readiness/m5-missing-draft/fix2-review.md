# M5 missing A1: fix2 residual review

No remaining finding in the scoped reader-selection fix or its delta. This is a static review only, not permission to launch or an M5 receipt. `READY_FOR_FAULT_COLLECTION` remains false.

The scenario uses the ordinary search options to select All admitted and records the accepted search summary before opening the note. The staged observer reports the actual `selected` option independently of the closed-omnibar search summary. Both click-boundary observations must report `None`, retain the same session, source generation and admissions, and belong to the A1 graph. The existing producer enumerates `saved_readers`, filtering only by `capture_selected`; that predicate returns true for every key when selection is `None`. Consequently, the Ready-reader comparison has complete saved-reader visibility at those observed boundaries. The scenario has no scope-changing input between them. The new mock case rejects restricted scope even when the hidden reader is absent from the projected list.

This is a bounded source conclusion: it does not establish delivery or behavior during a native run. The patch and Rust helper remain staged and uncompiled. The reported 13 Python tests and patch apply-check were not independently rerun. Source integration, review of the exact registration lock, matching app/test build provenance, a new intact collection, isolated fault run, ledger integration and headed frame review remain required. Preserve the lock and resource limits described in the prior review.

## Reviewed SHA-256

```text
9a34e978392d0765795b4c703709116001a2a746ed370422b62a5307e9d639e0  report.md
d6d9bb3434592c9cac8b3283d74cc2fcafa36deab2f6a3fc369b4ba94da16364  missing.scn
2222f771bf93c764864120af653f54748123b0973c5a23721afc91f5c71cf8c0  citation-observer.patch
6d7ee9825b99352f04f6f4899fecea2403c2020365d1cb3e43e7094ac7a54f3a  citation_event_history.rs
17d34360f18f358721ed8204bc386f3a822538b4f95cbc36ccf4c3d51f8b57a4  verify_missing.py
0c9484974353789ef6485ec86284744766f8faaa3d84df9170bda396ebb93383  fault_phase.py
22766745e295026c3ff8963d3cb87cdff84a99bd5fde1f41465f6fac83418b3c  run_missing.py
fc5553c4801dcfa8b021499bdf1747865c17956f8f4ab3afc6482153899b193a  test_missing.py
```
