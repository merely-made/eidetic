# M5 research worker wait correction

Collection G saved A and B, but timed out waiting for a current admitted research result; the unchanged presentation watchdog fired. During a scenario wait, `pump_research` drains producer intents, while search, comparison, and record worker responses had only been drained by the suspended normal actor event loop. G and its empty-cgroup cleanup are preserved at `/home/markik/Code/testing/m5-intact-collection-20261030-g`.

The bounded diagnostic `/home/markik/Code/testing/m5-research-diagnostic-20261030-a` allowed 120 ordinary frames and then observed a current accepted, complete search with one actual Capture hit. Three nonblank frames and empty cgroup were recorded; its final failure was an intentional stop before reopen. This supports the queue diagnosis without treating the diagnostic as full acceptance.

Frozen incremental patch `m5-research-wait-review-final/since-fetch-wait.diff` SHA-256 `5f6aad7cacf1c2d03824e1c152b768f22c99e5bd2c9946463c3454b04dd1be09`; full 36-file hash manifest SHA-256 `9333e5c4fb0e280f049ce66139f4144773464d72fa1dc82a2e0ea91939451516`. Existing three worker response handlers, ordering, and per-response custody service are shared between the normal loop and `wait_frames`. The watchdog and admission predicates are unchanged.

`cargo test --locked --offline -p turnstone --features authoring,inference --lib model_free_wait_drains_queued_research_worker_result -- --nocapture` passed 1/0 under the usual guard, log `m5-research-wait-green-2.log` SHA-256 `61b07cd51470038f23874709d673a5607678a13b3a9e843b113dd268c9e47586`. The regression starts the real research worker and session-owned trail actor, observes a queued response, maps it through the same search-response adapter, and requires App to accept it. It does not construct a headed Shell or test compare/records outcomes. Source-size and `git diff --check` pass. Red receipt `m5-research-wait-red-1` and first fixture-path failure `m5-research-wait-green-1` are retained.

Scoped review, new matching app and test-binary provenance, and another headed collection are required. This is not M5 qualification.
