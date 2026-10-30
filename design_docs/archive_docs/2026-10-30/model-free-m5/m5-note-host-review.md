# M5 Redshank host slice review

Verdict: changes requested. Review covers the 13 files named by `m5-note-host-review-hashes.json` against Turnstone HEAD `0089c9d331522343d81a69dcf1f3650ec662f4a4`, the producer brief, controls report, and captured diff. All 13 source hashes matched the review packet when inspected.

## Findings

### P1: Replay bypasses the scope check despite writing model progress

`src/app/redshank_notes.rs:92-116` allows `Replay` through the non-write branch without checking the rendered item, session, source-capture generation, or custody epoch. Yet `src/redshank_host/redshank_host_command.rs:58-73` seeks and plays the active transport, sets the command item's saved progress to zero, and reports a model change. When a replay issued on A is queued and selection, session, or custody changes before the shell drains it, it can affect the new active transport and persist a progress change in the current model under the stale command's item. This violates the requirement that queued writes be checked at application time.

Treat Replay as a state-changing operation and validate the authority it actually needs immediately before applying it. Add a regression that queues it under the old scope and changes item, session/source generation, and custody before drain, asserting no transport or model change. Preserve any intentionally supported selection behavior for freshly rendered controls with explicit tests.

### P1: Stale playback controls can discard a newer note draft

The same unchecked branch permits `Play` and the note-open commands after their rendered scope ceases to be current (`src/app/redshank_notes.rs:92-110`). For example, queue Play on A, switch to B and begin a note before the queue drains. Applying the old Play selects A (`src/redshank_host/redshank_host_command.rs:44-47`); `load_selection` clears all anchors and capture generations on the selection change (`src/redshank_host/redshank_host_retained_voice_preview.rs:200-202`), and the next projection clears B's draft (`src/redshank_notes_session.rs:103-111`). Note-open commands can likewise select another item when an audio runtime is available. Pause and seek can also control the newly selected item's transport. This gives an invalidated view authority over a newer capture and can lose typed text.

Bind commands with side effects, including navigation and transport, to the state under which their controls were rendered, or provide an equivalent explicit validation that permits an intentional current control while rejecting a stale one. Add a queued-event regression with two items and a live B capture, asserting stale A commands leave selection, capture generation, and draft intact.

## Evidence and limits

The captured source does use the supplier's listen panel, editor, privacy row, and compact dock; the hosted policy hides unsupported Edit and Delete actions. The explicit save/cancel path compares the current scope and distinct capture generation, and the host also checks the frozen anchor. The final combined log `/home/markik/Code/testing/custody-contract-20261029/m5-redshank-combined-3.log` hashes to `a02f5af695919d0e821e63de4c0e0e720fb32621eeeadc35dd1d9181f244ca51` and reports 30 passed, 0 failed, 1 ignored. It includes the retained-session event test, but does not cover the stale transport scenarios above. No tests or builds were run for this review.

The headed real-episode private refusal, sharing and ordinary retention, native provenance, restart, keyboard/accessibility, and viewport acceptance remain M5 work; this source review and focused gate do not establish those outcomes.

