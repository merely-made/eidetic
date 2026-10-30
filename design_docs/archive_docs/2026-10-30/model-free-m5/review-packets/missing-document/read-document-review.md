# Fault scenario Read document review

**Scoped disposition: no finding.** The only change from the previously reviewed scenario is `act Read document` immediately after `act Open research note`. The previous scenario hash is `ea8daeffe786581df4725c12515ac723c6413d6b11925edc05c66c064bc34c3a`; the new scenario hash is `542e00d4438ecc9348bd249ccc6aadcec1387577dfc777d253fa26bd773d8c90`.

`Open research note` selects the owning graph member and opens its address, but toggles its content only when `flip_spawns` is true. The scenario action resolves the ordinary palette label `Read document` to `SetWorkspaceMap(false)`, which reads the focused graph member. The existing content-live and citation-text assertions remain before the click. Actual citation visibility and the Peek-to-Read sequence still require the fault run and headed frame review.

The `missing_before_click` observation is taken after the added action. The verifier compares its event histories with `missing_A1_open`, so earlier note-opening events are part of the baseline and cannot satisfy the required one exact A1 reader-open event after the citation click. An extra address or reader-open event in that interval still fails; incomplete or overflowed histories fail closed. The all-admitted scope is separately checked across its earlier observation and the before-click boundary.

This is source review only. No build, test, or native scenario was run by this reviewer. The reported 17 tests are separate evidence. Keep `READY_FOR_FAULT_COLLECTION = False` pending the matching intact run and authorized fault collection.
