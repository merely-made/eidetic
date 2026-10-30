# M5 missing A1 fault draft: source review

**Disposition: changes required. Keep `READY_FOR_FAULT_COLLECTION = False`.** This is a source and containment review only. No build, native process, GUI, namespace, or test was run in this review. The eight reported Python tests are mocked/unit evidence, not proof that the Rust helper compiles or that the native case works. The separately reviewed acceptance collector does not approve this new fault driver.

## Findings

1. **The chosen intact state and owner evidence are not bound to the completed collection.** `run_missing.py:72-105,155-166` checks the phase-check JSON hashes and two integrity booleans, but the phase-checks contain frame dimensions and runtime filenames, not hashes of the citation, A1/A2 reader observations, `structured-inspection.json`, or profile tree. `regular_tree(root)` only snapshots the state as it exists when the fault driver starts. It does not compare the copied profile with the authoritative primary/inspection inventory from the completed inspection, nor validate the inspection schema, session, and canonical single graph against the observations. An intervening modification or wrong but internally consistent evidence can therefore be frozen and used for deletion. Bind the actual observations, independent inspector output and the pre-inspection authoritative profile inventory to a completed, validated intact ledger; verify the current tree against that inventory before copying and verify session and canonical graph identity independently. Preserve the post-exit and scope-cleanup checks.

2. **The reviewed helper is not connected to the binary provenance.** `run_missing.py:93-101,168-179` checks the collection's existing binary/build self-consistency, then separately freezes the current draft `fault_helper.rs`. Nothing proves those exact helper bytes, module integration and test registrations were included in that binary's source inventory at build time. An earlier binary can satisfy the existing provenance checks and the draft helper can change independently. After integration, bind the exact helper/module sources and integration to the source-before/source-after build inventory and the exact test binary used for a newly collected intact seed/reopen/inspection; verify the frozen binaries and those source hashes before the fault. Recheck compatibility against the changing inspector and compile under the agreed guard before any collection.

3. **The sharing check does not cover all capture receipts.** `fault_helper.rs:94-108` demands one A1 receipt matching its specific session/graph/node/annotation, and checks raw identities for `all_admissions` from the single graph. Another saved or unsaved receipt can name the same `raw_manifest` with a different request or annotation and be omitted from that graph inventory; the current `held.len() == 1` still passes. Deleting that key would damage its capture too. Before deletion, enumerate the complete relevant owner receipt/reference domain and fail if any distinct capture points at A1's raw payload; prove the graph/session scope is exhaustive or check the other relevant references as well. Include a counterexample test with an extra receipt sharing the raw key.

4. **Receipt preservation is compared only to the injector's own first store open.** `fault_helper.rs:93-100,123-128,175-182` hashes receipts after opening the copied Fjall store, then the separate postcheck compares to that hash. Since opening is explicitly allowed to perform recovery writes, this does not independently establish that the receipts equal the intact inspected state before the injection. Export a complete canonical receipt inventory or digest from the independent pre-fault owner inspection, bind it to the intact state, and compare both the injector's pre-delete and separate postcheck inventories to that baseline. Continue to compare the graph before and after the fault and preserve primary integrity on failures.

5. **The observed error alone does not exclude a fallback reader.** `fault_phase.py:37-50` and `verify_missing.py:33-50` require one matching A1 target in Error with no content, but permit other Ready readers/content in the same observation. They do not bind the ordinary citation click itself to the target it dispatched. A replay or fallback to a different target could coexist with the matching error and pass the data checks. Record and verify an ordinary-action observation carrying the exact clicked citation target and selector; fail on any successful fallback for that action. Keep the three frames as separately reviewed action evidence and require A2 Ready unchanged.

## Containment and qualification notes

The draft makes a fresh marked copy, rejects link/special-file trees, binds only named copy trees and frozen files, uses a dedicated namespace, and requests 3 GiB, zero swap, 150% CPU, 128 tasks and a 600-second scope. The copy's profile is deliberately mounted at two namespace paths for the saved note address. The supervisor uses its own child group/scope and attempts a primary inventory in `finally`; an unreadable or changed tree fails. These are source observations, not runtime verification. The launcher does not itself acquire `/home/markik/Code/testing/machine-heavy.lock`; the eventual invocation must hold that lock for the full scoped operation. Bind scope cleanup, mount identities, process order, binary/source, custody principal and route, graph, receipts, primary, and reviewed frames in the final fail-closed ledger. If GUI observation cannot establish the exact action, the case remains unqualified.

## Reviewed SHA-256

These hashes identify only the draft files read in this review. They are not build provenance or approval to launch.

```text
153b19202a38ae8e088a95ca41f2285f98c1d3bbe6eb84716db907fa8c64577a  fault_helper.rs
cdd0b810f41d6cc127db3a25cc2a13995f385f65692af496c54476f874192e48  run_missing.py
8223c6a4a23435b2aa116103dc3b069dbb1b93854935a1c78c13540b60285f9b  fault_phase.py
fa57ee0d62c25cdef8897ceb4d2c618e288c18b6c8036c33a45ce27f0555ba3f  missing.scn
952c11a6bb7afd6b7bbbd7d76663e932415836291051429521f0c96ba5224776  verify_missing.py
0c9e1d4024a1bd5e0c541dd3cfc32c08690c757e90bc9ff9ce08dd35062c7ffc  test_missing.py
e7fd5cbf5e9e0f98fdf2a05dc2ccfca328d182b5c010ec2179d49e7fed37e64f  report.md
4f4e046205849f78d3dea18a13b751ab85181d62e70e2b093607e79bfd0b1f21  integration.txt
bf14ab5d772c8a713d6e0473642e30922af180359de7bdbc58b9a4b8279aa29f  run_native.py
d6e55a189dc770f50c20245e7c2a687fce1c8563fa08a2bbac87c05618340c84  run_acceptance.py
fcac9ef1b535a3732db61ef8230415bc7f2cc26bef64c67abb988905ece87e26  namespace_phase.py
832655170652da3f8117bdc917c99a60d9ce94008afe930b4f4d60d5ba02f1db  acceptance_phase.py
```
