# M5 Redshank host fix 2 review

Verdict: **addressed**. No new correctness finding in the reviewed fix.

The fix distinguishes capture ownership from the control generation: same-item reopening changes the latter, so previously queued commands still fail the full scope check, while the unchanged capture retains its generation and editor draft. Session, source generation, custody, item, selection and capture changes still participate in capture invalidation. The retained-session test exercises typing, reopening the same episode, rejecting an old control, refreshing the projection, and issuing Save with the original generation and text.

The new test asserts the outgoing Save payload and non-empty effects; it does not inspect the model after Save. Because the command path emits a redraw even on host refusal, that final assertion alone does not establish that the body was persisted. The code path supports the expected save, but the receipt should retain that distinction.

Review covered `m5-note-host-fix2.diff` against the fix 1 review and the supplied 16-file hash packet. The parent verified the hashes and reports the focused gate at 34 passed, 0 failed, 1 ignored, log SHA-256 `3b6e6901e7f11bc1d61cb19937909a093aaf9c353eae25f06b39270e935dfb30`. No tests or builds were run for this review. Headed retention, device playback, restart, accessibility and viewport acceptance remain separate M5 work.

