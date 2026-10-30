# M5 native qualification harness

This bundle preserves the executable Python harness, scenarios, synthetic fixtures,
validator tests and build guard. Qualification remains pending. The reviewed missing-source driver has been enabled
and attempted; V failed its graph-inventory boundary. The original guard excluded
legacy `memory/` but included the sealed owner root. A red regression rejected an
intermediate exclusion of both roots; the reviewed guard excludes only the exact
`memory.sealed-sources-v1/` root. No corrected native fault result exists. The
final verifier remains DRAFT and deliberately refuses acceptance.
The sibling directory names are required by the pinned validator imports.
`source-manifest.json` binds each copied file to its original bytes.

The host is Linux with a user systemd manager, cgroup v2, bubblewrap, Python 3.9 or newer,
Rust/Cargo, `flock`, `/usr/bin/time`, an existing Wayland session and an accessible
DRM render device. Cargo dependencies must already be available for locked,
offline builds. The source checkouts and their exact dependency revisions remain
external inputs; the collector records complete source inventories and rejects
changed binaries or build receipts. This bundle contains neither application
binaries nor user stores or custody keys.

## Build and fixture preparation

`m5-draft/capture_build.py` records a guarded build and source inventory. Run its
`--help` to select the application, test executable or real custody test host.
The unchanged guard is preserved under `build-guard/`. Its original installation
path is `/home/markik/Code/testing/custody-contract-20261029/m2-queued-gate.py`.
Both scripts currently use `/home/markik/Code/target`; the guard uses
`/home/markik/Code/testing/machine-heavy.lock`. Another host must deliberately
adapt these paths, record the new script hashes, and produce fresh receipts.
Do not overwrite an existing guard or bypass the shared lock.

The HTML fixtures are synthetic. To reconstruct the synthetic audio in a new
fixture directory, copy the HTML files and decode `episode.mp3.base64` with
Python's `base64.b64decode` into `episode.mp3`. Check its SHA-256 against
`decoded_sha256` in the manifest. Pass that directory to `--fixtures`.
The collector removes the seed fixture server and its served bytes before cold
reopen and verifies that the old origin refuses connections.

## Native collection

`controller/p-launch.sh` preserves the exact collection P command as historical
evidence. Its paths point to that run's original inputs and output; do not rerun
it against the existing output. Use `acceptance-draft/run_acceptance.py --help`
with a new output directory, current guarded build receipts and inventories,
the bundled seed and reopen scenarios, reconstructed fixtures, and the actual
Wayland socket and DRM device. Supply a provenance document binding all three
builds. Existing provenance examples are retained with the collection evidence.

Run the collector under the shared heavy-work lock. For durable supervision,
use a uniquely named user systemd service with `Type=exec`, `RemainAfterExit=yes`,
`RuntimeMaxSec=1320s`, `MemoryMax=512M`, `MemorySwapMax=0`, and `CPUQuota=50%`.
Keep controller logs and start/final service metadata outside the collection
directory. Record InvocationID and MainPID immediately and verify the same
invocation's terminal state. The 1320 seconds is a deadline, not a guaranteed
completion bound. Each native child scope retains its independent 3 GiB,
zero-swap, CPU 150% and 600-second bounds.

An interrupted controller is a failed collection. Its separate native scope may
outlive the controller and released lock. Before any subsequent heavy work,
verify that exact owned native scope and cgroup are empty. Preserve the failed
evidence. An active service with ExecMainStatus=0 is not a successful run.

The collector's completion ledger only means evidence was collected. Inspect
the actual frames and independently validate the owner data, source identities,
versions, scalar selectors, complete record inventories and computed outputs.
Visual review records must name the actual reviewer, including Codex when used.
The copied integration README is historical review-packet text; its referenced
[fault-ledger review](../review-packets/fault-ledger/fix1-current/fault-ledger-fix1-review.md)
is retained separately. Historical integration prose saying "human review" must not be used to imply a
human inspected frames that Codex reviewed.

## Missing source and final receipt

After matching intact seed/reopen processes exit, the reviewed missing-source
driver operates exclusively on disposable copied stores. V attempted the fault and
failed the post-fault inventory check because the original guard excluded only
legacy `memory/` while including `memory.sealed-sources-v1/`. The reviewed guard
excludes only the exact sealed owner root and checks legacy `memory/`, graph data
and similarly named siblings. The original V collection remained unchanged. The
fault result is failed; no corrected native fault run has passed; final receipt
assembly stays DRAFT. Keep all sibling files in `m5-missing-draft/` together.

`m5-receipt-integration/assemble.py` copies ledger-bound evidence to a separate new
receipt directory and checks executable bytes while still available. Standalone
verification later checks their recorded bindings, not absent original binaries.
The final DRAFT guard must remain until the intact and fault runs, semantic
checks, visual reviews and independent final review all succeed.

The Python unit tests in each directory can run with
`python3 -m unittest discover -v` from that directory. They check harness behavior;
they do not qualify the native product.
