# M5 missing A1: fix1 source review

**Disposition: one finding remains. Keep the guard false.** This review is limited to the original five findings and fix regressions. It accepts the separate collector ledger delta review at `acceptance-draft/ledger-review/fix1-review.md` as the collector boundary. No tests, build, application, namespace, or native collection were run here. The reported 12 Python tests are not native evidence; the observer patch and fault helper remain uncompiled.

## Remaining finding

`verify_missing.py:45-55` compares only `readers` from the two saved-source observations. The producer at `src/shell/drive/shell_scenario_assert_no_surface.rs:335-347` filters that list through `research_scenario_privacy::capture_selected`; `citation-observer.patch` leaves this filter in place. Neither `missing.scn` nor the verifier proves that the selection is unrestricted at both observations. Thus an alternate Ready saved reader outside the selected scope can be omitted before and after the click, so the comparison can pass despite a new or changed hidden reader. Counting `AddressOpened` is useful, but does not by itself establish that no alternate reader can be created or changed through another internal path. Produce a privacy-preserving complete Ready-reader state or change indicator for the interval, or independently prove the relevant selection is unrestricted throughout; test the unselected-reader counterexample. Keep the exact event and headed-action checks.

## Original findings assessed

1. **Intact evidence and profile:** addressed at source level by `run_missing.py:90-110,116,247-250`, contingent on the separately reviewed collector's ledger creation and a new completed intact collection. The driver validates the ledger, profile baseline, inspector schema, session and graph before constructing input.
2. **Helper/build provenance:** addressed at source level by `run_missing.py:134-149,155-186,240-242,272`. The registration lock still requires actual human/source review, and the integrated source, module registrations and matching binaries must be built and collected together. Hashes and regex markers alone are not that review.
3. **Shared capture payload:** the complete owner receipt vector and all admitted graph captures are checked before deletion (`fault_helper.rs:36-53,157-171`). The staged regression includes saved/unsaved raw, body and stored-raw sharing. This is static evidence, pending compilation and execution with the actual fixture.
4. **Receipt preservation:** both injector and distinct postcheck compare sorted owner receipt count and BLAKE3 to the independent intact inspection baseline (`fault_helper.rs:24-34,159-164,239-244`). The independent inspector must actually supply this schema from the same profile and compiled source as the new collection.
5. **Ordinary click and fallback:** the bounded, redacted event history and exact interval now provide a specific dispatch observation; they require the same A1 target and selector and reject extra recorded address opens. The fallback coverage remains incomplete as described above. Frame review and actual event delivery remain runtime work.

The scope limits, copy isolation and `finally` integrity checks remain source-only. The full receipt verifier and reviewed frame-to-action binding remain integration work. The eventual invocation must hold the shared machine-heavy lock. Do not enable the guard on this review.

## Reviewed SHA-256

```text
5fad23e0d7faa91b55382d899257037b2a96f21ccd49fcfe68a4d4c31c93ade1  report.md
f676eca901815c608ff89f2ab3f6e945ba5a66ecc0c97e02f57f0864e378d134  integration.txt
892bd625a7aa1cf59211bf9ec425f59f4a63837daea22c405ecfc095a8b8d992  fault_helper.rs
76f65eaf7a4a591b4d042a15dc6eab97df37bc3a45eb85244516fe117f022b25  run_missing.py
d676f667993c8fcd360642434f3b48010b1b9bb248e202f54d06a2324b3dd5ec  fault_phase.py
c08aa2fb6b3be1a766d0964017766bc99ae1c931b6f29685e78791f3f604214f  missing.scn
9d5a057cf4129b690d78daf504bdb71b94255a63db2fd8a413d9c8cc7c4066dc  verify_missing.py
7b89cc60ea46271bbe03d4dfd4e3aae6a47e5b3aed0aa5bd425c13872a8e9023  test_missing.py
6d7ee9825b99352f04f6f4899fecea2403c2020365d1cb3e43e7094ac7a54f3a  citation_event_history.rs
7e6a0d88a77647ea1551f63ef3041b66f20cafa7a7e21e54ad81651d230b4fd8  citation-observer.patch
bf14ab5d772c8a713d6e0473642e30922af180359de7bdbc58b9a4b8279aa29f  run_native.py
d6e55a189dc770f50c20245e7c2a687fce1c8563fa08a2bbac87c05618340c84  run_acceptance.py
fcac9ef1b535a3732db61ef8230415bc7f2cc26bef64c67abb988905ece87e26  namespace_phase.py
832655170652da3f8117bdc917c99a60d9ce94008afe930b4f4d60d5ba02f1db  acceptance_phase.py
```
