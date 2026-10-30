# Final receipt integration harness

This historically named directory now contains the adopted final receipt assembler,
standalone verifier and 44-test helper suite. Its exact current file hashes are in
`../source-manifest.json`. The final W3 receipt and independent audit are recorded
in the canonical receipt under `design_docs/archive_docs/receipts/`.

The external manifest intentionally retains status `collected-awaiting-verification`;
the separate standalone verifier and independent audit report PASS. The receipt
binds 206 ledger-bound files and 62 reviewed frames. Original executable binaries
remain in the frozen collection. The assembler checked their hashes there; later
standalone verification checks the copied provenance bindings, not absent binary
bytes. The recorded reader-open event is target-bound but does not prove
request-ID causality. This qualification is limited to the recorded Linux source
cohort and does not establish non-Linux, hardware, personal-custody, power-loss or
general accessibility results. Model semantic quality remains 8/15.

Run the helpers with `python3 -m unittest discover -v` from this directory. These
tests verify helper behavior and do not rerun native scenarios.
