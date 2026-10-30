# M5 owner frozen-core source review

**Finding, important:** A claimed export queued for commit can be silently cancelled. `src/retained_clip/owner_worker.rs:228-230` skips a reserved job when its `Pending` is dropped, and `src/retained_clip/owner_worker.rs:125-130` drops all queued jobs on close. `src/retained_clip/owner_ops.rs:280-283` submits claimed export commits through that same path. This conflicts with the clarification that once claimed and queued, the write completes or fails and its actual result is reported. A deterministic regression should hold the worker, queue a claimed commit, drop its receiver or close the worker, then check the actual file outcome and lifecycle. Panic remains an explicit worker failure and must be handled separately.

The reviewed code otherwise establishes serialized physical owner access, policy and root retirement, bounded reservations held through response delivery and the prepare/commit handoff, and retirement ordering via the shared atomic. The included tests exercise overlap of two real reads, oversized records, policy retirement, dropped and undrained ordinary responses, panic, and export retirement/foreign claim/no-clobber outcomes. These source observations are not a complete acceptance of the whole feature or a substitute for shell integration review. In particular, the frozen overlap test does not itself demonstrate a real concurrent read and write, and the export test does not exercise overwrite success or the queued claimed-commit cancellation described above.

Before and after review, `sha256sum -c` passed for all nine files against the frozen manifest:

```text
6a5423dfb3adb96dc08d235b2793a92a5cb3897217ed11718656d8679637f722  src/custody_binding.rs
a7b924bb69c6e6a9271b80b6235b14c724046d9858cd9ed88a8009311d3d84cc  src/retained_clip.rs
1794933b9b11456ea9b3683ad1939e5c1383095f1b0adb286debabd33ef51b6e  src/research_storage.rs
04a39ab2fd03de488c5f3cbf6b4d06a350579c3dda313db1f3f671c2c592f7b0  src/retained_clip/owner_worker.rs
6f1c7e332e8bceff01b4b047162ac2f3f556c062a0b44e59188f192f849c4f9b  src/retained_clip/owner_ops.rs
10d1875d852b9200d7e518223cf8fdbd17666e1ae1a2f4ec556b6ba3d8068eb0  src/retained_clip/export.rs
bd5f789facd6c4b887dc4627edbd10cce59186a0a088e624f7dd1489e2fa7909  src/retained_clip/owner_overlap_tests.rs
80f838186b4309c62417ba5296a426374df968fcf7104c8f74cfeedd03d36e77  src/retained_clip/owner_worker_tests.rs
9cb0d489cb9a4d92d44f5fc7b0f76c1fa0b680418609fd73e945cae86b354e5c  src/shell/shell_start_private_connect.rs
```

This was source review only: no build, tests, native UI or runtime qualification was performed. The reported earlier 22 passed, 0 failed, 1 ignored gate predates the non-Unix portability adjustment. The reservation accounting reviewed here covers named payload bytes, not allocator usage or RSS; existing storage, decode and read budgets are separate. External revocation not yet locally observed cannot be ordered by the local claim authority.

## Targeted fix1 review

The prior claimed-commit cancellation finding is resolved in the reviewed source: reserved commits are marked must_complete, ignore receiver cancellation, and survive orderly close while ordinary queued jobs are discarded. The regression holds the worker, queues a claimed real-file export, drops its receiver, closes the worker, releases the hold, and checks the resulting file. No further concrete defect was found in this targeted correction. A dropped receiver cannot receive the outcome; the write itself still runs. Panic and process termination can interrupt the operation and are not a durable-queue guarantee.

The nine fix1 manifest hashes below matched both before and after this targeted source review. No build or test was run in this review. Subsequent changes require fresh review and hashes.

6a5423dfb3adb96dc08d235b2793a92a5cb3897217ed11718656d8679637f722  src/custody_binding.rs
a7b924bb69c6e6a9271b80b6235b14c724046d9858cd9ed88a8009311d3d84cc  src/retained_clip.rs
1794933b9b11456ea9b3683ad1939e5c1383095f1b0adb286debabd33ef51b6e  src/research_storage.rs
592db70f9d4366c6a0860606cb91764a3080476b23f4b22eaffae2343917c441  src/retained_clip/owner_worker.rs
6f1c7e332e8bceff01b4b047162ac2f3f556c062a0b44e59188f192f849c4f9b  src/retained_clip/owner_ops.rs
10d1875d852b9200d7e518223cf8fdbd17666e1ae1a2f4ec556b6ba3d8068eb0  src/retained_clip/export.rs
b905b6e07037d157a7ed5d252dcbc706503df451b9766fe6fe8c0b892fc4edc8  src/retained_clip/owner_overlap_tests.rs
80f838186b4309c62417ba5296a426374df968fcf7104c8f74cfeedd03d36e77  src/retained_clip/owner_worker_tests.rs
9cb0d489cb9a4d92d44f5fc7b0f76c1fa0b680418609fd73e945cae86b354e5c  src/shell/shell_start_private_connect.rs

## Targeted fix2 review

No concrete defect found in the revised checked named-payload arithmetic: retention reserves owned input plus body and record, and import and export reserve wire plus body and record, with root and relevant paths included. The prepared export retains its reservation through the claimed commit handoff. This is a payload bound, not an allocator or RSS claim. The claimed-commit fix remains present. Added source tests cover wire-only export reservation refusal, a queued real retention while a real read holds the owner, and successful replacement of an existing file. Tests were not run as part of this review.

All ten fix2 manifest hashes matched before and after this scoped review:

6a5423dfb3adb96dc08d235b2793a92a5cb3897217ed11718656d8679637f722  src/custody_binding.rs
a7b924bb69c6e6a9271b80b6235b14c724046d9858cd9ed88a8009311d3d84cc  src/retained_clip.rs
1794933b9b11456ea9b3683ad1939e5c1383095f1b0adb286debabd33ef51b6e  src/research_storage.rs
592db70f9d4366c6a0860606cb91764a3080476b23f4b22eaffae2343917c441  src/retained_clip/owner_worker.rs
1ea4c2a0d8033f58e3e15749fc8a0d9fc26d8c65486dcd5fcefc047cb7bee13c  src/retained_clip/owner_ops.rs
10d1875d852b9200d7e518223cf8fdbd17666e1ae1a2f4ec556b6ba3d8068eb0  src/retained_clip/export.rs
1f1c96baee0997c4f136e089ffcba76ba323c8e4e4a2faa05bac04a3161d34c1  src/retained_clip/owner_overlap_tests.rs
6f858c21aff9db76dfc27ec410c1c897921064fe8e72392d0895c61e18d44908  src/retained_clip/owner_write_overlap_test.rs
80f838186b4309c62417ba5296a426374df968fcf7104c8f74cfeedd03d36e77  src/retained_clip/owner_worker_tests.rs
9cb0d489cb9a4d92d44f5fc7b0f76c1fa0b680418609fd73e945cae86b354e5c  src/shell/shell_start_private_connect.rs

## Targeted fix3 review

No concrete defect found in the scoped change: prepared export matching includes its captured source root, a foreign worker rejects the reservation before enqueue, and retain/import report sync errors before success. Source tests cover mismatched roots and workers and a child process exiting without destructors followed by parent reopen. The latter does not establish power-loss behavior or prove the original lock failure's cause. The shell must invoke matching before claim; that integration is outside this scoped review. No build or tests were run by this reviewer.

All ten fix3 manifest hashes matched before and after review:

6a5423dfb3adb96dc08d235b2793a92a5cb3897217ed11718656d8679637f722  src/custody_binding.rs
a7b924bb69c6e6a9271b80b6235b14c724046d9858cd9ed88a8009311d3d84cc  src/retained_clip.rs
1794933b9b11456ea9b3683ad1939e5c1383095f1b0adb286debabd33ef51b6e  src/research_storage.rs
592db70f9d4366c6a0860606cb91764a3080476b23f4b22eaffae2343917c441  src/retained_clip/owner_worker.rs
6339ac7421cf4e0639c3a24d1fa50b13836463a4fc13aa4c7e2b310444e90937  src/retained_clip/owner_ops.rs
10d1875d852b9200d7e518223cf8fdbd17666e1ae1a2f4ec556b6ba3d8068eb0  src/retained_clip/export.rs
9ca003a44027e97f0ad3d130e3046622e9c8b6d64cdb4e4fb11934272abb4f05  src/retained_clip/owner_overlap_tests.rs
5929956d54a100b3ec5c4eb2a8541a617d4122ac58f0f7d4bb3ab790092009dc  src/retained_clip/owner_write_overlap_test.rs
80f838186b4309c62417ba5296a426374df968fcf7104c8f74cfeedd03d36e77  src/retained_clip/owner_worker_tests.rs
9cb0d489cb9a4d92d44f5fc7b0f76c1fa0b680418609fd73e945cae86b354e5c  src/shell/shell_start_private_connect.rs

## Targeted fix4 queue review

No concrete defect found in the scoped queue replacement. An admission begun before close remains counted through its send; the last admission wakes the receiver after close. Once closed with zero active admissions, the worker drains available messages before exiting. Checked atomic count and byte reservations roll back on refusal and are released on failed send or task/response drop. Ordinary queued jobs are cancelled on orderly close while claimed jobs drain; panic closes the service and disconnects remaining work. The standard unbounded sender does not wait for consumer capacity; accepted tasks remain limited by reservations. Source tests address pre-close admission sent after close, arithmetic overflow, and concurrent producer capacity. No tests were run by this reviewer.

All ten fix4 manifest hashes matched before and after review:

6a5423dfb3adb96dc08d235b2793a92a5cb3897217ed11718656d8679637f722  src/custody_binding.rs
a7b924bb69c6e6a9271b80b6235b14c724046d9858cd9ed88a8009311d3d84cc  src/retained_clip.rs
1794933b9b11456ea9b3683ad1939e5c1383095f1b0adb286debabd33ef51b6e  src/research_storage.rs
38c670f2aba7841312224c74da8f90931348217db9f68a24b0b5ed41b484846e  src/retained_clip/owner_worker.rs
6339ac7421cf4e0639c3a24d1fa50b13836463a4fc13aa4c7e2b310444e90937  src/retained_clip/owner_ops.rs
10d1875d852b9200d7e518223cf8fdbd17666e1ae1a2f4ec556b6ba3d8068eb0  src/retained_clip/export.rs
9ca003a44027e97f0ad3d130e3046622e9c8b6d64cdb4e4fb11934272abb4f05  src/retained_clip/owner_overlap_tests.rs
5929956d54a100b3ec5c4eb2a8541a617d4122ac58f0f7d4bb3ab790092009dc  src/retained_clip/owner_write_overlap_test.rs
8033d9c935dfb7bcd01c57f41c5c73eb5cdf549948fdcacd920d3a6b8635be4e  src/retained_clip/owner_worker_tests.rs
9cb0d489cb9a4d92d44f5fc7b0f76c1fa0b680418609fd73e945cae86b354e5c  src/shell/shell_start_private_connect.rs
