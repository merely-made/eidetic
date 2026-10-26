# stickleback

Shared replicated-space mechanics for signed peer domains in the
[mere](https://crates.io/crates/mere) workspace: the p2panda join ceremony and
receive drain, a muniment-backed operation store, policy-before-insert
processing, retention and epoch mechanics, group encryption state, and the
native drop carrier.

A domain supplies an extensions type, an `OperationPolicy`, and (where it prunes
history) a `CheckpointAuthority`. It keeps its own operation grammar,
addressing, authorization, and deterministic materialization. Stickleback never
infers authority from transport access or visible membership.

Consumed by `murm`, `mesh`, `gemot`, `moothold`, and `commons-spine`. It was
published as `murm-replication` through 0.1.0.

## Features

`net` is on by default. It is the peer-networking half: `JoinedSpace`,
`SyncedSpace`, `MunimentAddressBook`, the `Endpoint`/`Gossip` re-exports, and
`GroupKeyLane::join`/`publish`. It is the only feature that links `p2panda-net`
and, through it, iroh and QUIC. Everything else (the operation store and
processor, the causal index, group crypto and sessions, drop carriage, epoch
retention) is local, so a consumer that never joins a peer space depends on
stickleback with `default-features = false` and links no network. A consumer
that joins names `features = ["net"]` (or leaves the default on).

## Public surface

`address_book` (`net`), `drop` and `object_stream` are public modules; the other
surfaces are re-exported at the crate root, so a caller writes
`stickleback::MunimentStore`. The `Source` column names the source file, and
`(net)` marks what only exists with the `net` feature (see Features).

| Source | Contents |
| --- | --- |
| `synced_space` (`net`) | `SyncedSpace`, `SyncStatus`, `SyncRound`; the LogSync drain loop with real counters and a settle-based `resync` |
| `joined_space` (`net`) | `JoinedSpace<E>`; the join ceremony (session, live stream, subscription, drive) as one call |
| `lane` | `JoinError`, `lane_id`; the lane naming and join failures that need no transport |
| `store` | `MunimentStore<B, E>`, `BlobGcReport`; `OperationStore` + `LogStore` + `TopicStore` over a muniment `Backend` |
| `processor` | `OperationProcessor<B, E, P>`, `OperationPolicy`, `Admission<L>`, `StoreTarget<L>`, `HistoryAction`, `Reject`, `ProcessOutcome`, `ProcessError` |
| `authority` | `CheckpointAuthority`, the domain gate for retention trust-root transitions |
| `causal` | `CausalProjection`, `CausalEntry`, `CausalLimits`, `CausalError`, `PendingCausalOperation`, `causal_projection`, `observed_frontier`, `author_head`, `happens_before`, `validate_causal_metadata` |
| `drop` | `DropId`, `DropManifest`, `DropRecord`, `DropLimits`, `DropProtector`, `DropReadReport`, `DropWriteReceipt`, `ManifestEntry`, `EvidenceKind`, `NativeDropError`, and the `write_` / `read_` / `visit_` plain and protected entry points |
| `drop_io` | `DropExportProfile`, `DropExportSelector`, `DropExportBudget`, `DropExportDecision`, `DropExportStats`, `DropImportReport`, `DropFileExportReport`, `StagedDrop`, `DropIoError`, plus `export_topic_operations*`, `import_*_drop*`, the file helpers, and staged-drop and receipt management |
| `receipt` | `DropReceipt`, `DropReceiptLimits`, `DropReceiptError`, `ReceiptPeer`, `read_drop_receipt`, `write_drop_receipt` |
| `object_stream` | `ObjectStreamLimits`, `ObjectStreamError`, `read_object`, `write_object`; one bounded opaque object per asynchronous stream half |
| `epoch_retention` | `propose_epoch_pruning`, `EpochPruningProposal`, `EpochRetentionFacts`, `EpochCheckpointBasis`, `EpochHold`, `EpochHoldReason`, `EpochProposalBlocker`, `EpochRetentionReason`, `RetainedEpoch` |
| `group_crypto` | `DataKeyring`, `GroupCiphertext`, `GroupEncryptionMode`, `GroupEncryptionProfile`, `GroupCryptoError` |
| `group_session` | `GroupSession`, `GroupSessionId`, `GroupSessionDispatch`, `GroupSessionProcess`, `GroupControlFrame`, `GroupControlAction`, `GroupControlId`, `GroupDirectFrame`, `GroupPrekeyBundle`, `GroupRecipientId`, `GroupSessionError` |
| `writer` | `stable_writer_subject`, `WriterBindingError` |

Root also re-exports `p2panda_net::{Endpoint, Gossip}` (with `net`; what
`JoinedSpace::join` takes), `p2panda_core::Operation` (what
`JoinedSpace::publish` takes), and `p2panda_encryption::data_scheme::GroupSecretId`,
so a domain crate can call the join and publish seams without adopting a
p2panda crate itself. `VERSION` and `STAGE` consts are present.

## What a domain supplies

| Item | Role |
| --- | --- |
| `OperationPolicy<E>` | Addressing and authorization. Returns an `Admission<Self::LogId>` carrying a `StoreTarget` (topic + log id) and a `HistoryAction`, or a `Reject`. |
| `HistoryAction::Keep` / `PruneBeforeCurrent` | Whether the accepted operation retires the preceding prefix. `Admission::erasing_payloads` adds authorized body erasures. |
| `CheckpointAuthority` | `authority_revision()` and `permits_checkpoint(author, named_revision)`. Mesh uses an owner key; Moot derives a signer set from an accepted constitution. |
| An extensions type `E` | Carried in every operation header. |

`OperationProcessor::process` runs p2panda's structural checks, prune-aware
backlink validation, retained-frontier continuity, idempotence, and then one
atomic indexed write with optional prefix removal or authorized payload
erasure. `preflight` runs the policy without writing. `ProcessOutcome`
distinguishes insertion, payload hydration of an already-retained header, a
contentless duplicate, pruned entries, and erased payloads.

## Native drops

`drop` is the framing: fixed cover, semantic manifest identity, self-delimiting
records, streaming staged visit, integrity checks, configured allocation
limits, and an injected `DropProtector` seam. No domain key authority lives
here; murm injects an epoch-aware p2panda XChaCha protector. A suite that
decompresses must honor the codec's plaintext bound.

`drop_io` is the export/import path: topic operation selection, canonical
operation records, full-corpus structural and policy preflight, then ordinary
processor admission with idempotent reports. Domains inject omit / header /
full privacy decisions and settings-derived priority; stickleback applies a
deterministic semantic-byte budget and reports policy versus budget omissions.
Verified drops stage durably by `DropId`; operations, authorized prefix pruning
and payload erasure, stage cleanup, and a receipt commit in one backend batch.
Payload and blob chunks assemble by digest, and a header-only and payload-only
drop can meet later through a one-shot pending slot. Callers can `list_staged_drops`,
`resume_staged_drop`, or `discard_staged_drop`.

`receipt` carries a completion statement across the peer boundary. A local
atomic marker becomes a bounded, integrity-framed statement; received
statements live in a separate peer-scoped advisory namespace with
caller-directed cleanup. The peer service derives that scope from its
authenticated carrier identity, so a remote claim cannot become a local import
marker or bypass domain admission.

## Opaque object streams

`read_object` and `write_object` carry exact opaque bytes over a generic Tokio
stream. An object has a four-byte big-endian length, its payload and a clean
write-half end. The read checks length before allocation and requires EOF after
the payload; the deadline covers the entire call including FIN/EOF. Configured
limits must be nonzero and no larger than 64 KiB and 60 seconds. A caller may
choose a smaller profile; the mesh handoff example uses 16 KiB and 10 seconds.

This leaf does not decode operations, authenticate the carrier or mutate a
store. The receiving host selects the protocol/peer and concurrency/total
budget, then hands a complete object to its existing domain admission path.
An application acknowledgement follows that owner's result; transport receipt
and domain admission remain separate observations. The selected real QUIC
composition is documented in the
[mesh receive example](https://github.com/merely-made/mere/blob/main/crates/mesh/mesh/examples/retained_receive/README.md).

## Advisory retained-object inventory

The `inventory` module supplies bounded exact/Cuckoo availability summaries and
candidate selection. The caller owns retained bytes, namespace/read authority,
peer authentication, expiry and service admission. Summaries are advisory;
snapshot pagination and exact fallback remain explicit. The
[finite comparison](https://github.com/merely-made/eidetic/blob/main/crates/stickleback/examples/inventory_compare/README.md) records the policy,
initial bounds and current shared-ingress proposal with reproducible host inputs.

## Dependencies

| Crate | Why |
| --- | --- |
| `muniment` | The `Backend` trait and `WriteOp` batches under `MunimentStore`. |
| `p2panda-core` | `Operation`, `Header`, `Topic`, structural validation, prune backlinks. |
| `p2panda-net` (`net`) | `Endpoint`, `Gossip`, and the LogSync stream `SyncedSpace` drains. |
| `p2panda-store` | The `OperationStore` / `LogStore` / `TopicStore` traits `MunimentStore` implements. |
| `p2panda-sync` (`net`) | Sync event types on the drain. |
| `p2panda-encryption` (`data_scheme`, `message_scheme`) | Group secrets and DCGKA sessions behind `group_crypto` and `group_session`. |
| `identity` (`personae`) | `IdentityProvider`, `DerivedKeyAttestation`, signature types for writer binding and prekey bundles. |
| `proofs` (`mere-proofs`) | `Digest`, used for authority revisions and retention facts. |
| `blake3`, `hex`, `serde` | Drop ids, framing, and CBOR payloads. |
| `tokio`, `tokio-stream` | The drain task, status handles and bounded asynchronous object streams. |

## Examples and tests

`examples/ledger.rs` is a neutral domain showing the whole contract: an
extensions type, an `OperationPolicy`, and a `CheckpointAuthority`. Run with
`cargo run --example ledger -p stickleback`. `tests/boundary.rs` exercises the
same extension points through the public surface only, without referencing a
real domain crate.

## Status

Pre-1.0 (`STAGE = "pre-alpha"`). Live peer command wiring, the Moot domain
mapping, and a live Moot constitution fold remain. Mesh and direct conversation
supply concrete drop selectors today; murm's `ConversationEngine` runs on
`MunimentStore` for local authoring, gossip receipt, LogSync, and export
selection, with durable redb selection and reopen reconstruction landed.

## License

MPL-2.0 (see LICENSE).
