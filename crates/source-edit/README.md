# Source edit proposals

`mere-source-edit` prepares and checks UTF-8 text-edit proposals. It has no
parser, mutable editor, filesystem, save, undo or application-admission owner.
The first consumers are Isocosm's preset proposals and Knot's source-authored
local document links.

`Proposal::prepare(stamp, source, edits)` validates base-coordinate byte ranges,
sorts the edits, refuses overlapping/same-start ranges and produces a preview.
It retains the exact base source. `SourceSnapshot::capture(stamp, source)`
provides that same immutable observation to a host form before it knows the
replacement text. The form uses `check` instead of storing and comparing its
own address/text/stamp fields; `snapshot.propose(edits)` shares those captured
base bytes with the resulting proposal. It never recaptures a newer source. `application(current_stamp, current_source)`
requires matching document instance, revision and bytes, then returns candidate
text and a receipt with base/result coordinates and the next source stamp.
Neither call mutates a document or saves a file.

A host serializes check and application at its existing editor boundary. It
applies the entire proposal as one undoable transaction or refuses without
changing source, stamp or undo state. `edits()` is sorted in base coordinates;
replay needs descending offsets or equivalent simultaneous semantics, never
ordinary ascending sequential replacement. A host without batch undo should
accept only the single-range subset until it has a transaction boundary. It
assigns a fresh instance on open and advances the source revision for every
text change, including undo/redo. Equal text after undo does not revive an old
proposal. A receipt describes the candidate transaction; only the host can
report that it was applied, admitted or saved. Stamps are local observations,
not durable domain identities or authorization tokens.

Keep the existing editor as the sole source buffer. Knot preserves its own
selection, composition, undo and file guards; Isocosm preserves its typed schema,
TOML ownership checks, snapshots and explicit saving. Format-specific proposals
must pass their consumer's validation after shared range validation. Neither
Rowan nor a TOML policy is imposed on other consumers.

The proposal is not deserializable: deserialize strict `TextEdit` inputs, then
prepare them against the host's observed stamp/source. Untrusted serialized
previews and receipts cannot bypass preparation. Hosts still own source size,
history, authority and platform limits. Distribution uses the canonical Mere Git
source with committed consumer locks; hosts may select `main` or a published
revision when a separate source identity is needed to preserve existing pins.
`publish = false` prevents a crates.io release; it does not prevent this Git
distribution.
