# M5 shell, Bridge and scenario source review

Review: independent static review of the 22-file shell/Bridge/scenario freeze in `/home/markik/Code/worktrees/turnstone-model-free-m1-20261029`. `sha256sum -c m5-owner-shell-freeze.sha256` passed for all 22 entries both before and after inspection. The manifest SHA-256 was `f221f0846d7dda157b6c1d4d9b129091cdadf1e77a6b5095e4e67accad8b6465`; its entries give the exact reviewed file hashes. The worker plan SHA-256 was `a60c967037b9842f0f1869944f9beb234c6e541b57146afdd0316a290a1778bf` and claim-placement clarification SHA-256 was `a26931981e102192033ce1df18f26bf4888b96d264700570bcdf6e6e4145973e`.

## Finding

A pending retained voice Play can start after Stop. `src/shell/effects/shell_effect_open_theme_workshop.rs` handles Stop immediately without invalidating an outstanding Play request. `src/shell/clip_actions.rs` plays the delayed result whenever the original clip and scope remain current, which Stop does not change. Thus a user can stop the clip while its owner read is queued and hear it start afterward. Invalidate the corresponding pending Play on Stop and cover that ordering.

## Reviewed boundaries

Shell creates one owner and attaches it to Bridge; production read, retention, import, export preparation, and voice playback dispatch through that handle. Shell jobs use the independently held private binding and revalidate captured scope and custody before publishing. Bridge rechecks context and epoch across its background wait. Reader completion checks the citation and selector through `finish_research_clip_document`. Export preparation is validated against current scope and its destination, followed synchronously by the bound claim and commit enqueue; claimed completion reports the actual outcome. Native waits drain owner completions, and retention status ties the wait to a fresh dispatch and resulting manifest. Direct retained-clip Fjall helpers observed in `retained_clip.rs` are test-only.

This is source inspection, not proof of runtime behavior. Worker core, reservation lifecycle and concurrent custody retirement were assigned separately and were not independently reviewed here. Builds, tests, native scenarios, real file ordering, and hardware qualification were not run. Locally unobserved external custody changes remain outside the claim guarantee. The finding requires correction and a refreshed freeze and review of changed files.

## Fix 1 scoped re-review

The fix1 manifest SHA-256 is `ce494f2cf3024533db9735f349cdba987e3f016c0a0970eca73f7d273b3d649f`; all 22 entries passed before and after this re-review. Stop removes all matching pending Play receivers before stopping playback, preventing a subsequently drained completion from starting that clip. The reader wait now admits a direct retention only with the current successful retention, matching focused target, live retained engine and no pending read; an existing citation must independently pass its strict currentness check. Prepared-export matching includes the captured source root. No additional concrete blocker was found in this scoped source inspection. The focused tests were inspected but not run by this reviewer; the guarded gate and runtime scenarios remain pending.
