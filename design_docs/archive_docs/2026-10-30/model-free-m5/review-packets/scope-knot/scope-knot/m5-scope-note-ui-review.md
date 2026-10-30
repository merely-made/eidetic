# M5 scope and note UI review

No critical or important findings in the scoped changes.

The scope effect change invalidates stale structured state before issuing the replacement search. The shell applies effects in order, so the old cancellation precedes the new worker submission. The focused test checks both all-sources and empty chosen scope. The original red run was not on frozen source and remains bounded as described in the packet.

Knot scroll nudges the existing pane offset, renders to clamp it to the current content extent, and uses the same retained viewport offset for hit testing, pointer coordinates and text caret selection. The native wheel enters the existing content input route. The CSS constrains the columns and wraps long unbroken text; the changes do not alter source bytes, retention identity or selection export. Actual native rendering, wheel interaction and passage retention remain to be proven by the planned scenario. This review ran no builds or tests.

Reviewed Turnstone HEAD `c2f69b743a0cb3d637b32fb2ef7f4c164fb60d8a`; the two-file scope diff matches packet SHA-256 `05928228ff9a9f5aeb93c681d175b952ccb0d06f633f0e1e9e08157484ef906c`. Exact SHA-256 file manifest:

```text
273c66f7482ab62c3c865d3e2a0585c734db52f342783b36822fb6a5057f7460  src/app/research_record_scenario_tests.rs
968638d93ac8c1f344e1323803673a8499aebcf902d03f0974c4042d226bee8d  src/app/research_search_arms.rs
720413fac99ac60cf82d5fe29d62929ab08bda5c15a25956405e41f242b532b1  src/knot_authoring.rs
3f2bd04ecd7b0cc0fac7206740a4b40e8c1557d7554ed3e7a3ca07e17cb400b9  src/knot_authoring/impl_document_session_scene_for.rs
5a38f81f3fb91a73444b6805debe9edc66a343dbe9623575381683fef3dab72f  src/knot_authoring/knot_document_session_pointer_event.rs
ae0ca1b259eec0879fe9a43be08b21d6ab8a5240548a97f139d8260437d73ccb  src/knot_authoring/knot_sheet.rs
07321da515a1c364db421e2126d8269e251abc83f371b7b5360780f64d911eb5  src/knot_authoring/tests.rs
cfe904ce5a59b1823301676144f46adf71bde37b5114708f1f174b5f0710c431  src/knot_authoring/tests/note_scroll_cases.rs
```
