# Custody copy fix review

Scoped verdict: **approved as a source change**, not authorization to enable or run the fault driver. `READY_FOR_FAULT_COLLECTION` remains false. Runtime intact and fault receipts remain required.

Reviewed SHA-256: diff `eac434b9a5a0d82d26fab7146091559af6aace42e81ef1c5e714b463c1e07011`; runner `260d7e33b73d3abafea742007e150b3be46f12721f3669366d7e472989b046c5`; tests `e3877487244a7b9c75c2166ed6e86bc3e8588223cebd5669d8bf45d35ddd5dc2`.

`collection_tree` inventories the original collection, rejects symlinks, hard-linked regular files and unrecognized nonregular entries, and records device, inode and mode for only canonical per-run custody sockets. An outer `app.sock` is rejected. `copy_custody` validates the fixture root and runtime directories, copies only the persistent data, vault and fixture files into separate inodes, and does not copy sockets. Its existing fixture reopen path validates the synthetic wallet and profile. Unknown runtime contents fail before launch.

The original inventory is taken before copying, checked again before launch, and compared in `finally` even on failure. A missing initial inventory or changed original fails closed and records failure evidence. The namespace writes to the separate copied roots. These checks are not a guarantee against concurrent replacement between filesystem operations; actual original preservation and successful reopen require runtime evidence.

The change leaves `verify_bound_case` unchanged and its only non-builtin global dependency is `verify_missing`, which the integration verifier supplies when AST-extracting it. The integration verifier still pins the previous runner; parent must update the runner pin and dependent assembler pin after review and verify the resulting artifacts before any integration claim.

Independent lightweight check:

```text
$ python3 -m unittest discover -s m5-missing-draft -p 'test_*.py'
......................
----------------------------------------------------------------------
Ran 22 tests in 0.106s

OK
```

No native execution, build, guard change or pin change was performed.
