# Producer extraction bounds source review

Scoped root review; no build or runtime qualification.

- `src/producer_retention.rs` SHA-256 `4f64d7501f5582eeb75f79da6e1eb949e59151eccba189e0a77e42407a5a9297`
- `src/knot_authoring/knot_document_session_pointer_event.rs` SHA-256 `6ef91b5e9c28d70832bd9465182d5e2cc4987466b88d48763ba6c5ccc5c64b51`
- `src/knot_authoring/tests/saved_selection_from_embedded_pool_pointer_gesture_cases.rs` SHA-256 `dd44529f8f49b82c7bf5f99b6852e51f0239bfe4af491d81df15e8d08ae64e38`

The checked pre-copy admission inspects Knot backing length, exact byte range and source address before constructing the owner export or copying complete backing. Redshank serializes borrowed complete annotation and library item into a capped counting writer before invoking the owner export. This includes representation and fingerprint fields. The conservative wire-size allowance accounts for source URL expansion and JSON escaping; it is not an allocator or process RSS bound. The digest writer preserves the existing serde JSON stream without creating the full temporary vector.

Supplier review: Woodshed pin b3dfacff selected_export.rs and annotation_source inspect only the counted annotation/library source; local file URL and item fallback strings derive from those fields. Knot pin055e1f4e owner anchor/export files are unchanged versus the inspected primary source. This review does not cover a future supplier change.

Initial rejection tests used budgets that would reject an ordinary small note; revised fixtures first admit the small equivalent at16KiB, then independently enlarge metadata, fingerprint, local path or enclosure URL and reject under that same limit. Large-budget exports must equal the owner export. The source has the meaningful controls; their execution remains assigned to the sole heavy coordinator.

No remaining blocking issue identified in this narrow source slice. Caller migration, request reservation, custody, write/export authorization and final compile/runtime behavior remain outside this review.
