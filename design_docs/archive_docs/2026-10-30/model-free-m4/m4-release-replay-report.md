# Fjall replay release issue: v2 applicability

## Finding

Fjall 3.1.13 includes the fix for [#332](https://github.com/fjall-rs/fjall/pull/332), released on October 25, 2026 ([release notes](https://github.com/fjall-rs/fjall/releases/tag/3.1.13)). The upstream report is that startup replay restores already-flushed journal entries. A later flush can then produce overlapping L0 runs; in the upstream FIFO path, the non-disjoint L0 violates FIFO's assertion and panics, poisoning subsequent writes. The PR's `recover_does_not_replay_flushed_items` fixture exercises the important sequence: flush records while they remain in the active journal, reopen the FIFO keyspace, add more monotonic-key records, flush again, assert one L0 run, and make another write. It is materially stronger than an expected-values-after-reopen check.

## Pinned v2 source comparison

The repository pins Fjall 2.11.2 and lsm-tree 2.10.4 (`support/patches/fjall/Cargo.toml:15-16`, `support/patches/lsm-tree/Cargo.toml:15-16`). In ordinary v2 recovery, the active-journal loop inserts every item into its tree without consulting the tree's highest persisted sequence number (`support/patches/fjall/src/keyspace.rs:687-712`). Sealed-journal recovery also inserts all items first, then compares only the journal's last per-partition watermark to the persisted sequence number before deciding whether to clear or rotate the recovered memtable (`support/patches/fjall/src/recovery.rs:146-201`). Therefore duplicate replay is present in this v2 source. In the partial-coverage case, old persisted items and later unflushed items can be in the same recovered memtable and later flushed together. This can also refill recovery memory, which matters to bounded replay accounting.

The specific upstream FIFO panic is **not source-proven for this pinned v2 engine**. V2's FIFO strategy does not assert that L0 is disjoint: when it sees non-disjoint L0, it delegates to maintenance compaction (`support/patches/lsm-tree/src/compaction/fifo.rs:99-108`). That maintenance strategy does nothing until L0 has more than 20 segments; above that it merges selected L0 segments (`support/patches/lsm-tree/src/compaction/maintenance.rs:57-77`). The compaction worker also has a general path for scanning selected segments from a non-disjoint level (`support/patches/lsm-tree/src/compaction/worker.rs`, `create_compaction_stream`). I found no v2 `L0 needs to be disjoint` assertion in these paths. So the newer release's exact FIFO panic and poison mechanism cannot be claimed to apply to v2 based on source review. I did not run an isolated runtime reproduction; whether a different v2 failure exists is UNTESTED.

## Eidetic default path and existing receipt

Muniment opens its partition with `PartitionCreateOptions::default()` (`crates/muniment/src/fjall_backend.rs:68-74`); v2's default compaction strategy is Leveled (`support/patches/fjall/src/compaction/mod.rs:40-44`, with the partition default at `support/patches/fjall/src/partition/options.rs:320`). On recovery, Fjall decodes and retains the partition's persisted options. Muniment does not expose FIFO configuration in this open path. Thus the specific FIFO poison is not the expected behavior of a newly created default Muniment store. An externally pre-created or otherwise custom FIFO partition is outside that default-path conclusion.

The existing `partial_flush_across_partitions_preserves_acknowledged_values_on_reopen` test checks four expected values after a partial flush/restart, but sets `compaction_workers(0)` and does not cause a post-reopen flush (`crates/muniment/src/fjall_recovery_tests.rs:128-151`). It does not exercise the upstream FIFO failure and does not disprove duplicate replay. No new fixture was added and no build or runtime test was run, to avoid interfering with concurrent recovery and storage work.

## Warranted next step

Controller decision: preserve existing v2 replay semantics for this milestone and charge all physical journal entries, including duplicate replay. Per-sequence pruning may be assessed separately with its own correctness proof. The configured recovery bounds do not require that optimization. Do not port a v3 fix or mark the v2 FIFO poison reproduced. If runtime characterization becomes necessary, coordinate with the recovery and primary agents, then use a standalone fixture against untouched registry Fjall 2.11.2 / lsm-tree 2.10.4 and preserve its output separately from the active patched-engine receipts.


## Release status refresh during M5

A fresh GitHub API check on October 30 still reports 3.1.13 as latest. The web
`releases/latest` alias returned an older cached 3.1.12 page, so the direct release
and API response govern this refresh. Write-buffer issue #344 and strict-recovery
issue #311 remain open; the linked [strict-recovery PR #337](https://github.com/fjall-rs/fjall/pull/337)
is open and unmerged. These statuses do not change the v2 applicability findings
above. The separate M5 overlapping-open regression concerns consumer ownership
of the same exclusive store; no fix for that is listed in 3.1.13's release notes.
Metadata is preserved in `../model-free-m5/release-status-refresh.json`.
