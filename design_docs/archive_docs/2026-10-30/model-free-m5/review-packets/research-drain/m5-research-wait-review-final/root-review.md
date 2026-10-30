# Shared research completion drain review

Approved for scoped integration. Root verified all 36 source hashes and the focused real-worker receipt (1 passed, 0 failed). The two-file delta extracts the existing search, comparison and record response drains, preserving their order, custody checks, app reducers and effect handling. Normal actor processing and bounded scenario waits now call the same helper. The helper does not manufacture results or bypass request/scope admission.

The regression observes a real worker completion queued before the App sees it, applies the shared search response mapping, then observes an accepted current result and an empty queue. It does not construct a headed Shell. The prior bounded native diagnostic established search acceptance during ordinary frames; fresh app/probe builds and the next full native run are still required. No watchdog, budget or source-authority change.
