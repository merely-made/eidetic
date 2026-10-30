# M5 fetch wait integration review

## Result

Approved for the scoped source integration. No correctness finding in the shared fetch-drain extraction, wait call, or timeout diagnostics.

This is a static source review. It does not establish a fresh build, native collection success, or M5 qualification.

## Verified scope

- Turnstone `HEAD` remains `293eb1038a343b9068a9a73746b096b22c053bf0`.
- All 36 source paths in this packet's `hashes.json` match their listed SHA-256 values.
- `hashes.json`: `3fa6be04e7d579f0dc7f33351a9baaf937a0cf07b4dfc8d0b90770bae7a108ea`.
- `since-readiness.diff`: `531ed42582409275e27dabb1a9093d5c0f3a91eaaaf3347128619e67dd1490b0`.
- `git diff --check` passes for the changed scenario and fetch-drain files.

## Fetch drain and wait

`drain_fetch_updates` extracts the existing loop from `drain_actor_updates`. It retains the same processing for subresource completions, `browse::update_from_fetch`, app updates, and effects. `next_fetch_update` polls `fetch_rx` first, then `nomadnet_rx` only if the first poll yields no item, matching the former `try_recv().or_else(...)` selection. The regular event-loop drain now calls this method, so both paths share the same fetch handling.

`wait_frames` calls `drain_fetch_updates` on each bounded poll before research, trail, and place drains, then advances retained documents and Knot state as before. This lets a queued ordinary page response update the app during `wait-source-target`; it does not synthesize or dispatch the capture action. No watchdog code changes appear in the packet.

## Diagnostics and tests

On timeout, the diagnostic distinguishes no browser command target, a target URL mismatch, a source-admission refusal, and a current admitted source whose action is absent or non-unique. It uses the same browser command target and source-admission query used by the action catalog, and does not print a different target's URL.

The focused regression opens an ordinary address, obtains its generated fetch request, then feeds correlated progress and terminal updates through the new fetch-selection helper and the existing fetch-to-app reducer. It verifies the capture action remains unavailable after progress and becomes available only after the terminal response. This exercises the producer correlation and reducer sequence, but it does not instantiate `Shell::drain_fetch_updates` itself. Source inspection confirms that extracted method invokes that same selection and reducer path. The target diagnostic tests cover the no-target, admission-refusal, and different-target cases in the reviewed delta.

The reported passing test receipts and in-progress provenance builds were not rerun here. Fresh build provenance and an intact native collection remain necessary before claiming native readiness.
