# Fjall background worker lifecycle source review

No concrete defect found in the scoped change. All four production call sites register a worker before attempting thread spawn. The captured worker is dropped if spawn fails; unstarted drop, ordinary return and panic unwind each drop the activity before the final completion guard decrements the count. Keyspace teardown observes the count with Acquire and retains the existing stop signal and repeated flush/compaction wake path. The change does not alter external snapshot and iterator lease ownership.

The focused test source checks pre-start registration and closure drop, and holds activity destruction after a stopped worker actually starts to observe the count and lease before release and join. No build or tests were run by this reviewer. This source review does not establish the actual interleaving behind diagnostic B, nor prove all crash or hardware outcomes.

The following hashes matched before and after review:

```text
8e76bd7bbe165826bda6066a1da73a5a59737daba24d7d6d356d327689c310a6  support/patches/fjall/src/background_worker.rs
9e1d31f0428499ccddba04b829320cbd5063511702c8abbc9566a5b9a2dbc058  support/patches/fjall/src/keyspace.rs
521441fcb53646b710f88297ea94fd357aba5372855fcb32fd00588f80b4faaa  support/patches/fjall/src/background_worker_lifecycle_tests.rs
```

## Test configuration follow-up

The only further keyspace change sets `flush_workers(2)` in `force_flush_multiple_partitions`; its assertions and production behavior are unchanged. The three source hashes matched before and after this scoped review. The revised keyspace hash is `5496583de6dffe4bb1f8433b9dba8e0ad98cccfda2b571f44eca408069bb9f59`; the other two hashes remain as listed above. No tests were run by this reviewer.
