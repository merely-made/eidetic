# M5 limit refusal observer review

## Result

One finding blocks approval of the clearing semantics. The refusal is only cleared on invalidation when another structured projection is present. The rest of the scoped observer path is source-consistent with a real current query failure.

This is source review only. The reported focused receipt hash matches the packet's cited log; I did not run tests or builds. A native worker limit hit remains unverified.

## Finding

**Medium: scope invalidation can leave a refusal stored and make it visible again after the scope is restored.**

`structured_error` clears prior refusal state, then the matching query failure stores `(request, error)`. On a query that has no `view_scope` or other structured projection, `invalidate_structured` returns early when `pending`, `view_scope`, `recovery_scope`, `discovery_scope`, and `history_scope` are all empty. That guard does not consider `limit_refusal`, so the later stale-scope check never runs for this case.

The new scope-current check in `structured_scenario_status` correctly hides the refusal while selection/settings/context/custody/graph scope differs. But scope selection changes call `invalidate_structured`; if selection changes away and then back to the original value without submitting another structured query, both calls can hit the early return. Once the scope again matches the stored request, the old refusal becomes observable again despite the intervening invalidations. This is especially reachable after a first query on an otherwise empty structured controller, where `view_scope` is absent. The focused test's graph-change assertion checks that the status is hidden, then submits a new query (which clears the refusal); it does not check that invalidation cleared the stored refusal or that it stays cleared after restoring the original scope.

The early return should account for `limit_refusal` (or the invalidation event should clear it before that return), and the focused regression should cover changing scope away and back without a new query. No source change was made in this review.

## Other scoped checks

- The completion handler first requires `structured_response_current`, which matches the response to the currently pending full `Job::Query` and checks its request scope. Only a current `JobResult::Query` whose worker error string equals `Structured record limit exceeded` stores a refusal.
- The status output requires the structured view to be visible, the controller not busy, the same error still present, request selection/settings equal to the captured scope values, and a current context/custody/graph scope. Stale state is therefore hidden while it differs.
- New submissions, structured commands, and structured errors clear the stored refusal. The gap is the invalidation early return described above.
- The status path reads the held request and app state; it does not submit a query or write research records. It includes query, settings, selected member identities, graph IDs/revisions, and refusal text, but does not serialize candidates, stored records, source bodies, or evidence payloads. Output cardinality follows the saved selection and current graph list; there is no separate response-byte cap in this observer.
- Packet identities verified: `since-research-wait.diff` SHA-256 `84197610846d53508b1f6a2408158ff0d6fc3881f6c2c9e0559fbe7a17b21ecd`; `hashes.json` SHA-256 `9072bf8ba80ad139d0226edf348625f2693a9b138640603a429df087d4620a8d`. All 38 listed source hashes match the current checkout at HEAD `293eb1038a343b9068a9a73746b096b22c053bf0`.
- Focused receipt `m5-limit-refusal-green-2.log` SHA-256 matches the supplied value `042f745ab06b455c99ababb7beee432b07a1d2e6740e9fd9564a4aa62131cb3e`.
