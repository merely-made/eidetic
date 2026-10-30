# M5 readiness fault integration review

## Result

Approved for the scoped source integration. No finding in the new source-target wait, seed delta, or fault observer/helper registration plumbing reviewed here.

This is a source review only. It does not establish native readiness, collection success, fault execution, or M5 acceptance.

## Verified scope

- Current Turnstone `HEAD` is `293eb1038a343b9068a9a73746b096b22c053bf0`, matching the packet.
- All 34 source paths in `m5-readiness-fault-review/hashes.json` match their listed SHA-256 values.
- `hashes.json`: `42841c359f7a6bba1b21fa724db954e288a5e137ff04c09417f39f28e059db1c`.
- `since-fix2.diff`: `1d127413a5a32c9fd2519b8ccb1d7cce4c497b22acb92e40ac7915063f8a4814`.
- `seed-since-run.diff`: `bd471a461bcbacc23018497acc99818263db5c4e6356485fe2604ce7274a97c2`.
- The three cited source-test receipt hashes match the files in `testing/custody-contract-20261029`.

## Source-target wait

`wait-source-target <frames> <exact-url>` parses a required unsigned `u32` frame count and exactly one URL token; missing, malformed, or extra fields return a parse error. Environment expansion happens before parsing, so `${M5_ORIGIN}/A` becomes the literal target URL, and an unset variable errors with its line number. The parser test covers valid syntax and missing/invalid frame input.

The driver dispatches this as a wait step. Each poll calls `app.available_actions()` and checks for exactly one action whose displayed label is `Save source and keep` and whose variant is `CaptureSourceDocument` with `target.url == url`. The inspected UI plumbing builds that action from the Inspector's ordinary `CaptureSourceDocument` intent, while `source_capture_target` derives the URL from the current graph member. The predicate only observes that action list; it does not synthesize or invoke an action. Zero or duplicate matches remain not-ready.

The driver uses the existing `wait_frames` loop: it polls at least once (`frames.max(1)`), at most the requested count, advances the existing background drains between checks, and returns a descriptive scenario error on timeout. The count is caller-supplied `u32`, with no additional practical ceiling; this seed uses 600 frames. The subsequent three-frame settle and normal `click-row Save source and keep` preserve the ordinary click path.

## Seed and registration

After `New research note`, the seed now waits for the existing local note address prefix `local file:///m5/run/profile/notes/research-`, then retains `assert content-live`. For A/B/A it waits for the exact loopback URL, settles three frames, and then clicks the offered row. This addresses the prior click-before-offer condition without injecting product behavior. The ten-second presentation watchdog is unchanged.

The staged scenario module plumbing is present: `WaitSourceTarget` is part of `Step`, parser and scenario tests are registered through `scenario.rs`, and the new step is routed through the same scenario wait-step dispatcher as other waits. The approved fault observer/helper source remains integrated with its registration modules in the packet; the address-history and shared-refusal test receipts are reported as source tests only. I did not repeat the helper's broader fault review.

## Qualification boundary

The packet records the initial collection failure and does not claim a successful retry. Native waits, note-root setup, exact action availability under the live app, saved-source semantic verification, fault-helper execution, and M5 acceptance remain unverified. Fresh provenance and a new intact collection are still required before native qualification.
