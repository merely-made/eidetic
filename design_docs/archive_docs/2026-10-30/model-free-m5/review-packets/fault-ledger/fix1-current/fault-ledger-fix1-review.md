# Fault ledger fix1 review

No remaining finding in the two scoped fixes. Static review only; the fault guard remains false.

`complete_collection` now requires all three executable names in the pre-launch inventory and compares the full regular frozen tree, including executable contents, to that inventory twice before publishing the ledger. The reported regression cases cover changed scenario bytes, changed binary bytes, and a missing binary. `execute` now calls `finish_collection` after primary integrity; that wrapper records the completion exception and time in `failure.json` and re-raises, so completion validation failures cannot produce a successful ledger. As with any receipt, a filesystem write failure can itself prevent persistence.

All five files match `fault-ledger-fix1-hashes.json`; `run_missing.py` is `f7eb35b99c97c9c609cc6018fda524e6e35ecfeb1f9d695378fbe764e28c5762`. The reported 17 tests and Python compilation were not independently rerun. This review does not qualify integration, builds, the native fault run, or headed evidence.
