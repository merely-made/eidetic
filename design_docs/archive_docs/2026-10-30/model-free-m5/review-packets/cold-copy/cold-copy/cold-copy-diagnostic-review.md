# Cold copy diagnostic review

Verdict: **do not execute this revision**. Static review only; the wrapper was not run and no native or build work was performed.

Reviewed SHA-256:
- `cold-copy-diagnostic.py`: `723e812dec5e8082ea92176d0781b748cd4d649e7ca6057b1111d22a3120a8e0`
- `cold-read-diagnostic.scn`: `03bc5f00e5eee2f361e06278a4ca2b697d4a4ca8552cadecb5cfe3ac571c8fc0`

Blocking findings:
1. `validate_reader` accepts the expected string anywhere within any nested string of `rendered_document.blocks`. Additional text, including the private surrounding selection, passes. Compare the complete rendered text/body to the expected body and reject extra text; test a body with the expected text plus an extra block.
2. The search hit is matched to the reader only by node, version and selector. Session, graph and manifest are checked against the reader's own admission, not against the accepted hit's retained source identity. Bind all identity fields to the accepted hit and reject independently mutated session, graph, manifest, node, hash and selector, including inconsistent search/open sessions.
3. The observation enumerates all live retained reader nodes (`shell_scenario_retained_readers.rs`), so a matching previously open reader does not establish that the chosen suggestion is the currently displayed reader. Require evidence tying the displayed/active reader to the hit, or explicitly report that the diagnostic cannot establish that fact. Test a wrong active reader alongside a matching stale reader.

Boundary review: the wrapper inventories the three copied trees with `tree_hashes`, which rejects symlinks and nonregular entries, copies only those trees and listed files into a new marked output, omits the stale custody socket, checks source inventory again in `finally`, freezes the listed binaries/helpers, and uses the reviewed namespace and bounded cgroup helpers. The namespace binds the new sandbox and frozen inputs, with a separate network namespace and no personal home mount. Cleanup uses an owned unique scope and requires empty-cgroup evidence. These are static observations, not a runtime isolation receipt. The individual `FILES` paths and source parent components are not independently checked for symlinks before `copy2`; reject symlinks and nonregular files there and ensure source path components are plain directories before copying, so the stated copy guard is complete. Source inventory covers the specified trees/files and frozen inputs, not all of Q.

The scenario uses ordinary Read document for A1/A2 and Escape before Ask; screenshots still require visual review. The intentional failure and diagnostic-only status do not establish M5 acceptance. The final integration verifier remains mandatory and unchanged. Q remains an immutable failed collection; its successful seed and partial cold observations do not constitute full acceptance.

## Scoped fix re-review

Reviewed frozen SHA-256: wrapper `a35976695590563227af2990035a4fb69a251388c4f573e30718466aa9538141`, scenario `75795cf20f3c4b2f94f0a98ca1f489db6029df229ace1dac12417ffd1b93b98c`, test `49abf80300bfdf8d09796c7e31372f77e2ffab7b40aa72ad9b249abcde73f3c4`.

Verdict: **the scoped findings are resolved for a diagnostic run**, subject to the explicit visual-review limit below. This is not acceptance or a broader re-review.

- Full body: exact expected quote and paragraph, headings, export and bounded synthetic metadata are checked; extra body and metadata canaries are rejected by the test.
- Identity: the accepted retained URI binds session, graph, node, manifest and hash to both admissions and the reader; selector and engine are checked. Mutated identity tests reject mismatches.
- Active reader: the validator explicitly records `active_reader_proven: false` and `visual_review_required: true`; a matching coexisting reader therefore is not presented as proof of the displayed reader. Same-case fresh frame review remains required.
- Copy paths: plain directory components and regular files are checked with `lstat` before inventory/copy, including the previously unguarded individual files. Symlink-parent and nonregular-file tests reject their fixtures. This is not a concurrent mutation guarantee.

Independent lightweight check, from the SDD directory:

```text
$ python3 m5-draft/test_cold_copy_diagnostic.py
....
----------------------------------------------------------------------
Ran 4 tests in 0.009s

OK
```

The diagnostic wrapper was not executed. The intentional stop, copied state, and visual-review requirement constrain any resulting evidence; the separate final integration verifier remains mandatory.

## Custody copy delta review

Scoped verdict: **approved for diagnostic retry**, subject to the existing visual and acceptance limits. This is a static review; the diagnostic was not run.

Reviewed SHA-256: wrapper `a244f9e24068cdf03883391ed0367ead2d70f7d627107f2ae9abd9372e4ac7a5`, tests `af3c22dbf926349518145ba93936d914a61f6cacd9cc214fff1fd41d94a2fff6`, scenario `75795cf20f3c4b2f94f0a98ca1f489db6029df229ace1dac12417ffd1b93b98c`.

The copy preserves the existing profile, Mere, custodian data and vault trees, initialized marker, client metadata and fixture markers, while omitting the old socket paths. The socket inventory accepts only canonical `run-UUID` directories containing one socket and records device, inode and mode for those and the top endpoint; unknown custody state entries fail closed. Source tree/file hashes and socket identities are checked again in `finally`. The copied sandbox is the namespace write target; Q is not mounted. This does not guard against concurrent replacement between checks.

The fixture source checks the initialized marker, reads the existing identity seed, wallets, roster and epoch bridge from the custodian data root, and opens the existing synthetic profile before serving. Thus the copied persistent roots match the source's reopen path; successful reopen still requires runtime evidence. Original socket identity and persistent-state preservation likewise require the resulting integrity receipt.

Independent lightweight test:

```text
$ python3 m5-draft/test_cold_copy_diagnostic.py
.....
----------------------------------------------------------------------
Ran 5 tests in 0.009s

OK
```
