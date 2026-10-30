# M5 cold reader and limit invalidation review

## Result

Approved for the scoped source delta. The earlier limit-refusal invalidation finding is fixed, and I found no stale/private target or body export in the new retained-reader projection under the frozen all-sources scope.

One boundedness note is recorded below. This is a source review only; the added cold reopen sequence has not been qualified in a headed cold run.

## Limit-refusal invalidation

The early-return guard in `invalidate_structured` now includes `limit_refusal.is_none()`. A refusal-only controller therefore reaches the stale-scope check and clears its projection on a selection scope change. The added regression changes selection away, invokes invalidation, restores the prior selection, and verifies the old refusal does not reappear. This closes the reviewed roundtrip gap.

## Retained reader target and body

The observer walks the actual live content nodes and requires a stored `ClipReader` for each retained-clip engine node. The target returned is the exact `reader.target`, gated by:

- target node equals the current live node;
- target session equals the current session;
- reader generation equals the current source-capture generation;
- reader identity is the current identity Arc;
- current custody epoch equals the reader epoch;
- the exact `ClipAdmission` remains in current-session graph admissions;
- the node still exists in the target graph at the manifest-derived retained-clip address;
- source selection is all sources (`None`). An explicit subset suppresses both target and rendered document.

The rendered body is copied from the currently live `SmolwebDocumentSession` after these gates. No sealed clip is reopened and no store read, search, or mutation is introduced. The target is the live reader's stored target, not synthesized from a hit.

The search summary independently requires an accepted, idle, current search request and matching query/settings/selection. It projects each accepted hit's source, version, producer, and the passage's scalar start/end selector. It does not export passage text; the fixture tests this with a body canary. These fields are references from the actual accepted hits, while the reader target/body come from the actual current live reader.

The added reopen steps first record and capture each live Selection/Note hit, then use ordinary `choose-suggestion` controls to open it, settle, assert live content and expected text, and record/capture the reader. This preserves an inspectable before/after record around the actual UI action. The source delta does not establish that the cold reopen succeeds in a headed run.

## Boundedness note

`scenario_retained_readers` recomputes `research_clip_admissions()` inside the loop for every live retained reader. That rescans current graph nodes and placement checks once per reader, making observation work scale roughly with readers times graph nodes. It does not read clip bodies or write state, but the admission list could be computed once before the loop and reused to keep the observer bounded more directly.

## Verified packet

- Turnstone `HEAD`: `293eb1038a343b9068a9a73746b096b22c053bf0`.
- All 39 paths in `m5-cold-reader-review/hashes.json` match their listed hashes.
- `hashes.json`: `1f52f6fd222c7cff0778f5db5ed78af89909e0fef4fe6049b635b7a5e7f34170`.
- `since-limit-observer.diff`: `1908fc7945bb3c4de34a300bd19b704bb31d061fc64f5752fa521a348da7387b`.
- `m5-cold-clip-reopen.diff`: `fd13784941463fa073639234f9e33e1d65caf2230a9cf9b5138e1d0f42871637`.
- The focused test logs match the report: limit roundtrip `8ba8d961ebbd230142624abe5ab6a0a171f6b073fc6cc7df984d1e94ab4c3b1d`, retained-reader scope `41c33eb620f7ea608ca25259a5776b7ddfc9f5a41c676876c8f26c8a94b0ebc0`, and hit-selector scope `616cec165e10ba7e7011fc63633acd3d0f9be851948d0fc3182669693e3219e3`.
