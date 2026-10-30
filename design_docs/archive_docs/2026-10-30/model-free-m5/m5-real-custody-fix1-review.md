# M5 real custody fixture, lease fix review

Verdict: **approved for the scoped source review**. The earlier lease-lifetime finding is addressed; no new issue was found in the fix delta.

The fixture-feature-only `Service` field owns an `Arc<File>` for the locked inode. `Service` derives `Clone`, so the ticker, signing bridge and session workers retain the same lease; the returned `Fixture` also owns the service. A listener accept or session-thread spawn failure therefore cannot release the lease while those service owners remain. The child-process regression forces an accept failure, drops the original service, checks that a separate process cannot acquire the lease, and checks release after the child exits. This tests the failure underlying the review finding.

Reviewed `m5-real-custody-fix1.diff`, SHA-256 `e8a71ff0acc9163159c9ae5bc3e108eb6a57efe0e3721120a02f16df8cea2322`. The live four-file hashes match `m5-real-custody-fix1-hashes.json`:

- `ports/djinn/custodian/src/fixture.rs`: `e0e0459a73b6c107e52bd15269f85526905167dadc6f49d36eda39ff3260cd7d`
- `ports/djinn/custodian/src/driver.rs`: `b86cde6d355721885f42c094de1414fbaf3a5006e99669f1f24749feb837ea31`
- `ports/djinn/tests/custody_payload.rs`: `c6ee9d37aab9b37a637281ba76bb67f60ffac6fa49c4ad7a9171ca24f1c83eaf`
- `ports/djinn/tests/custody_receipt_tests/mod.rs`: `b23b1730a7782330111ef81e315766a274dba543d579ef845032e12c758671f7`

The parent verified the recorded logs: the corrected listener-failure regression passed 1/0 and the post-fix lifecycle, profile and damaged-state tests passed 3/0. The first regression attempt failed during setup with a missing vault and remains recorded. I did not run builds or tests. Actual application use and M5 offline-restart qualification remain separate.
