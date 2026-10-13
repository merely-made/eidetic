# Advisory inventory policy and finite comparison

Stickleback's [`inventory`](../../src/inventory/mod.rs) module produces bounded
availability hints for opaque retained objects. The caller owns original bytes,
retention, namespace/read authority, authenticated peers and delivery admission.
The leaf stores metadata, generates summaries and selects bounded offers. It
has no network, object store, operation decoder or domain admission callback.

## Run

From the Mere repository root, after workspace dependencies are cached:

```sh
python3 crates/stickleback/examples/inventory_compare/run.py /tmp/inventory-comparison-new
```

The output directory must be new. The runner builds the current Cargo-reported
example executable and executes it twice in independent processes. Reports
must be byte-identical. It saves one complete report, grouped summary, build/run
logs, toolchain, source/lock/executable hashes and replay evidence. The selected
[comparison](results/20261012/comparison.json), [summary](results/20261012/summary.json)
and [provenance](results/20261012/metadata.json) are exploratory observations.

## Policy contract

An `Inventory` accepts owner-supplied whole-record digests, lengths and expiry.
These digests are distinct from canonical signed operation IDs. Announce retained
objects with `retain`, forget owner-selected evictions with `remove`, and call
`expire` with caller-owned monotonic time. Capacity refusal leaves metadata
intact. The owner must bound actual bytes and recheck availability before sending.

A `Snapshot` freezes at most 64 IDs for pagination. `SummaryMode::Exact` returns
the first eight-ID page; continue through `exact_page`. `SummaryMode::Cuckoo`
builds a new filter from that immutable set. If construction saturates, it
returns an exact page and marks `saturated`. No partially mutated filter escapes.

`PeerInventories` retains at most eight peer views and ages idle state. Its
`observe` method assembles one exact snapshot, rejects mixed pages, validates the
completed snapshot digest, and handles duplicate pages idempotently. Incomplete
exact state produces no offers. A filter produces probabilistic hints; the host
must schedule exact fallback even if that filter hides all useful offers.
`offers` enforces count and payload-byte limits, skips expired entries and rotates
its cursor across decisions so repeated lost replies do not starve later IDs.

Authorization occurs before decoding/storing a peer's summary. Inventory-read
permission is separate from permission to submit a record. Reset a peer view when
the owning authenticated context changes. A stale but structurally valid summary
can temporarily misguide offers; its revision is a snapshot identity, not an
authenticated replay counter or a custody promise.

## Version-one control profile

All numbers are explicit little-endian; no Rust `Hash` or `DefaultHasher` is used.
An envelope is `MIV1`, kind byte, 32-byte namespace, u64 revision, 32-byte snapshot
digest and u16 total count. Exact kind zero adds u16 cursor, u8 page count and up
to eight sorted 32-byte IDs. Cuckoo kind one adds 128 little-endian u16 cells.
The decoder enforces a 1 KiB ceiling, known version/kind, bounded counts, exact
length and sorted exact IDs. Empty exact pages only represent an empty inventory.

The filter has 32 four-slot buckets and nonzero 16-bit fingerprints. BLAKE3
derive-key contexts `mere.inventory.cuckoo.key.v1` and
`mere.inventory.cuckoo.alternate.v1` define indices/fingerprints. The first
fingerprint uses hash bytes 0..2 as little-endian, mapping zero to one; the bucket
is hash byte 2 masked by 31. The alternate offset is its derived byte zero masked
by 31 and ORed with one, then XORed with the bucket. The bounded builder uses at
most 128 deterministic kicks per insertion. Snapshot digests use
`mere.inventory.snapshot.v1` over namespace, revision, count and sorted IDs.
This is an authored inventory profile, not MemoryLAN wire compatibility.

## Experiment and shared ingress budget

The finite eight-node ring compares eager push with eager-plus-exact repair and
eager-plus-Cuckoo repair with exact fallback every fourth repair round. All modes
have the same fresh forwarding path, 4,096-byte directed round budget and two
shared receiver reservations per four-tick round. Repair modes reserve a repair
opportunity after allowing one fresh opportunity, then reuse idle capacity for
fresh work. Source order rotates. This scheduling choice is part of the
comparison, not a claim that summaries alone explain every delivery difference.

Seven scenarios across five seeds and three modes exercise a fitting 32-object
finite workload, 256-object churn, combined loss/partition/restarts, eight-object
recovery, all-cache restart with/without a surviving 32-object retained source,
and expiry before partition healing. Churn publishes one object per tick; that
is sustained overload during publication for this receiver profile. The fitting
workload publishes one per eight ticks. Fault scenarios use 25% loss. Individual
restart discards volatile owner state; the observer retains historical receipt.
The cold-source case restores only node zero's retained originals after restart.
Historical receipt and final retention are separate metrics.

Inventory and retained payload are capped at 64 entries and 1 MiB per namespace,
with 16 KiB per object. Fresh pending ID queues are capped at 64 per direction;
those are experimental object queues, not network session queues. Each snapshot
exchange has a 32-tick deadline. All actual encoded summaries, modeled continuation
requests and offer/request envelopes include four-byte framing. Payload attempts
reserve their four-byte prefix and a 52-byte modeled result frame. Complete
digest-verified reception emits that result; lost results do not reverse receipt.
Failed attempts consume reservations and byte budgets.

The coordinated resident-ingress proposal has four queued sessions per service
ALPN, two active submissions shared across peers, 16 KiB objects, 2 KiB results,
and host-owned complete-call deadlines. **That service/API is not implemented by
this slice.** The harness's two reservations model its proposed active bound;
it does not qualify the session queue or handshake. Live inventory needs its own
authorized protocol path; the future Notochord-admitted service has a different
handshake from the object-first QUIC example. A candidate list is not a transfer
permit. The initial Retinue producer may retain only four objects/64 KiB while
the policy supports its larger maximum.

The later [exact live composition](../../../mesh/mesh/examples/inventory_ingress/README.md)
implements that proposal with the four-object producer, separately admitted
inventory/submission actions and the existing resident endpoint/mesh owner.
Its actual session, signed-byte and decoder evidence is separate from this
finite policy comparison.

The existing [`read_object` / `write_object`](../../src/object_stream.rs) helper
can carry bounded opaque messages. Live dispatch must acquire owning service
capacity, retrieve current exact bytes, verify their whole-record digest, then
freshly decode and invoke the existing operation/domain owner. Selected live QUIC
and signed-operation evidence remains with the
[mesh receive example](../../../mesh/mesh/examples/retained_receive/README.md).

## Evidence boundary

This harness exercises the actual production inventory module with synthetic
opaque payloads. Control exchanges are synchronous bounded substeps; payloads
arrive one tick later. It records useful unique receipt, missing/expired demand,
duplicates, actual summary bytes, modeled framing/requests/results, queue/state
bounds and state-trace hashes. It requires fitting/small-set recovery in repair
modes, recovery from the surviving cold source, and empty caches after global
state loss without a source. Churn completeness is measured, not required.

It does not exercise sockets, QUIC reliability, signatures, MeshStore admission,
RaptorQ, RF, physical airtime or MCU resource limits. Slots have no seconds or
Mbps conversion. The older patched MemoryLAN report remains a separate protocol
baseline with different workload, schedule and bounds; its numbers are not a
paired comparison with this harness. This implementation is downstream-owned;
no upstream maintainer submission is part of this work.
