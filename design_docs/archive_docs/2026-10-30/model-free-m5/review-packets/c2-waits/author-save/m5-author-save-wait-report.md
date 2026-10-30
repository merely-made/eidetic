# Author save wait review packet

Collection L timed out at the existing 450-second seed limit after `authored_4.png`; its owned cgroup is empty. This is a collection timeout, not a finding that property save failed. The 14 authoring actions each previously used a fixed 300-frame delay. The scenario delta substitutes `wait-structured-author-save 600` after each real `Save sourced property` selection, with no change to the scope or resource cap.

The read-only predicate requires the visible Author view, no draft or controller error, no busy or pending job, and a current `view_scope`. `structured_submit(Job::Deposit)` clears that scope; `ConfirmProperty` restores the current post-commit scope after the real owner and graph save. The actual preceding selection must succeed: an app-step selection failure still fails the scenario. This is a wait barrier, not independent evidence for the number or distinctness of retained records. The separate owner inventory remains required.

The wait pumps the existing normal research worker drain. If the app reports an error, it exits the wait and includes that error in the failure. A stale, hidden, busy or pending state cannot satisfy the predicate. There is no product record mutation in the observation.

- Source diff `m5-author-save-wait-source.diff`: SHA-256 `deb7dab6ef7ea546e5e686a7fa32e826808542ef82bc4c5e8286fdede32f8aab`.
- Seed diff `m5-author-save-wait-seed.diff`: SHA-256 `7112e9493362882dd175593cbaf40cbfff49fe3ac2dd4f76310536b79c4cb26d`; resulting seed SHA-256 `6b619a3bbf68188e2d2f6931708b1a084465d8ecdeadbcb53388e444e511802b`.
- Red `m5-author-save-wait-red-2`: one actual owner-backed author integration test failed because the new status field was absent; log SHA-256 `961d7920e5c56e04b766f53d27d900b5c8653a696b7c8f2e23622eefe015d1d2`. Red-1 accidentally selected zero tests; preserved and not counted.
- Green `m5-author-save-wait-green-2`: the same owner-backed test passed 1/0, including submit, confirmation, wrong view, hidden view, both error channels, pending, busy, stale session and restored scope. Log SHA-256 `e2ca72ce10382fadb81de79a5244b43e167e73482ac5f5fc9a93e7174649f5d5`. Green-1 failed to compile the new test due to an unwrapped SessionId and is preserved.
- Parser gate `m5-author-save-parser-1` passed 1/0; log SHA-256 `5b3a38888c97095d0b2a0c97c125f95b4f6b490566cb7861da30595278cab3e1`. `git diff --check` and source-size check passed.

Source is frozen for scoped review. The wait and full M5 seed/reopen remain untested in a native run. Other fixed waits around comparison and withdrawal lack the same confirmed-save predicate and are unchanged pending real evidence.
