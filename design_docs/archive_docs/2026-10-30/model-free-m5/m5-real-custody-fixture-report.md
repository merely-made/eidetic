# M5 real custody process fixture

Mere base: `a4cbd4baf3411b597bdcfc13653de0444b5d480d`. No production admission, dependency pins, root locks, commit or push changed.

`retained_custody_receipt_host` is the existing ignored integration-test entry point in `custody_payload`, built with `custody-receipt-host`. The environment remains `EIDETIC_CUSTODY_FIXTURE_ROOT` and `EIDETIC_CUSTODY_FIXTURE_ENDPOINT`; the latter must equal `<root>/app.sock`. Root must be an absolute canonical private owned directory with `fixture-marker` equal to `eidetic-retention-20261019\n`. A new root may hold only that marker and the stable lease file. The first run creates a synthetic vault and wallet; later runs require initialized state, check the wallet and profile, then use ordinary RemoteHost unlock. Core assigns its own fresh instance and generation. A stable root lock stays held by the service thread; per-process unique runtime directories do not unlink old listeners. Socket cleanup remains the supervising caller's responsibility.

Receipt `m5-real-custody-focused-1.json` and log under `/home/markik/Code/testing/custody-contract-20261029/`: focused process test passed 1/0; earlier frozen snapshot, exit 0, log SHA-256 `f5339a2e4d2c1ef5b3e43314e7355868aa83d8f9c7d427d969f7b6545d300325`, max RSS 2,424,916 KiB. It proved a different real instance, stable public principal, sealed payload reopen, stale instance write refusal, and byte-identical vault/wallet files.

Receipt `m5-real-custody-focused-2.json` and log in that directory: three tests passed 3/0; earlier frozen snapshot, exit 0, log SHA-256 `dd52c16f06e6b5c0749f0632c1b32ae4b8710803305d3e383d93562e9e5b9f34`. Added incomplete wallet refusal and stale write after profile roundtrip. Existing `custodian_client::real_custodian_fences_private_payload_to_authoritative_generation` covers lock/unlock. Both gates used the shared `machine-heavy.lock`, systemd MemoryMax=3G, MemorySwapMax=0, CPUQuota=150%, one Cargo job, `--locked --offline`.

Final source hashes, frozen through the following gates:
- `ports/djinn/custodian/src/fixture.rs`: `326677d8aa2792db74f0da01e757203a7d3504fa77601cd6d68b6c55e92cb62c`
- `ports/djinn/tests/custody_payload.rs`: `c6ee9d37aab9b37a637281ba76bb67f60ffac6fa49c4ad7a9171ca24f1c83eaf`
- `ports/djinn/tests/custody_receipt_tests/mod.rs`: `b23b1730a7782330111ef81e315766a274dba543d579ef845032e12c758671f7`

Final `m5-real-custody-focused-3.json` and log: 3 passed, 0 failed, exit 0, log SHA-256 `9a09b55cf484c4bcaea9701c57e91875e7c34d15ba612f109f937254c899f8e7`, maximum RSS 1,972,544 KiB. This gate included profile identity verification before unlock and corrupt-vault refusal. Final `m5-real-custody-lock-fence-1.json` and log: 1 passed, 0 failed, exit 0, log SHA-256 `3132ca7d5583e2264451ef3a3c990d10b9e4f501b53407d07c1bd3b641baab36`, maximum RSS 1,967,624 KiB. Its exact command was `cargo test --locked --offline -p mere-resident --features custody-receipt-host --test custodian_client real_custodian_fences_private_payload_to_authoritative_generation -- --exact --nocapture`. The final fixture command was `cargo test --locked --offline -p mere-resident --features custody-receipt-host --test custody_payload custody_receipt_tests:: -- --nocapture`. This is fixture evidence only, not M5 presentation, offline-restart, or product acceptance. No retained-research content was seeded.

## Review fix: service failure lease

Round-one review found that the root lease was owned only by the listener thread, which could return while the broker and custodian workers remained alive. The fixture-feature-only `Service` lease now follows every service clone, including ticker, signing bridge and session workers; production builds do not contain the field. A child-process regression injects a failing listener, drops the original service, checks that a second process cannot lock the inode while workers remain, and checks that process exit releases it.

The first guarded regression `m5-real-custody-lease-fix-1` failed before the intended assertion: the child did not create a vault, so Core returned `vault-missing`. Its retained log SHA-256 is `e3cdcadc9d8bb49584fe471c9e6229b4aeb6596155d8ecd5847c5f1677956f22`. The child now creates a synthetic passphrase vault and profile.

Review-fix source hashes, frozen for the new gates:
- `ports/djinn/custodian/src/driver.rs`: `b86cde6d355721885f42c094de1414fbaf3a5006e99669f1f24749feb837ea31`
- `ports/djinn/custodian/src/fixture.rs`: `e0e0459a73b6c107e52bd15269f85526905167dadc6f49d36eda39ff3260cd7d`
- The two receipt host and test files retain the hashes above.

Corrected `m5-real-custody-lease-fix-2`: 1 passed, 0 failed, exit 0, SHA-256 `0817d5775dee447ee67e3d8310f45d7ee39a2fa495a6c3594aad91bafd1ea16d`; maximum RSS 525,520 KiB. Command: `cargo test --locked --offline -p djinn-custodian --features fixture --lib fixture::lease_tests::lease_survives_accept_failure_until_service_workers_exit -- --exact --nocapture`.

Post-fix `m5-real-custody-post-lease-1`: 3 passed, 0 failed, exit 0, SHA-256 `7d747271d485b8b1f1d7836f7d12e2d23a2fdfff894831d17dcab4de3544ac66`; maximum RSS 2,413,560 KiB. Command: `cargo test --locked --offline -p mere-resident --features custody-receipt-host --test custody_payload custody_receipt_tests:: -- --nocapture`. Both used the shared machine-heavy lock, 3G memory, zero swap and 150% CPU quota. `git diff --check` passed after both. Scoped review is pending.
