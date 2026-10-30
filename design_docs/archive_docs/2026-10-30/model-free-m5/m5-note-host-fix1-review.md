# M5 Redshank host fix 1 review

Verdict: changes requested for one new draft-loss regression. This review is limited to the original two findings and the fix captured in `m5-note-host-fix1.diff`, with the 15-file hash packet and correction receipt as supplied.

## Original findings

- **Stale Replay writes progress: ADDRESSED.** Replay is classified as a write and all admitted commands compare the rendered control scope with the current scope before application. Replay also requires a present custody epoch and the selected item. The added regression covers item, item-return, capture-return, session, source-generation and custody transitions. Its transport observations are from a silent-host test, not an active audio device.
- **Stale playback controls discard a newer capture: ADDRESSED for the reported scenario.** Play and note-open commands now require the same current control scope; selection and capture transitions advance the control generation. The retained-session regression queues A controls, creates a real A note and types a B draft, then checks rejection, B's generation, and the preserved draft. Freshly rendered Play on an inactive item remains supported.

## New finding

### P1: Reopening the currently selected episode cancels its draft

`src/redshank_host/redshank_host_retained_voice_preview.rs:209` advances `control_generation` for every `load_selection`, including when the item is already selected; the code directly below deliberately retains the capture for that same-item case. Reopening the current audio address follows `open_redshank_episode` → `open_item` → `select` → `load_selection` (also reachable through `src/app/node_arms/app_commit_gemini_identity.rs:160-161`). The shell compares the entire old control scope at `src/shell/events/shell_drain_actor_updates.rs:212-215`, treats this new generation as capture invalidation, and cancels the still-matching capture through `src/app/redshank_notes.rs:210-223`. The next projection clears the editor at `src/redshank_notes_session.rs:106-114`. Thus reopening the same episode while a draft is present loses the text even though the item, capture generation, session, source generation and custody remain current.

Keep stale controls invalidated while distinguishing control-context changes from capture-owner invalidation, and preserve the editor when its capture remains current. Add a retained-session regression that types a draft, reopens the same episode, runs the shell invalidation/projection path, and verifies the same generation and text survive while commands issued before the reload are still rejected as appropriate.

## Evidence limits

The supplied final correction log reports 33 passed, 0 failed and 1 ignored, SHA-256 `dc6b3307baeb23846b6c5c12e2f93c2421397c58f5e1f3b47da8e073eb20bd12`; the parent verified the source and log hashes. No tests or builds were run in this review. Headed retention, device playback, restart, accessibility and viewport acceptance remain separate M5 work.

