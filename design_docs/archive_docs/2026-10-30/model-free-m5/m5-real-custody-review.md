# M5 real custody fixture source review

Verdict: **changes requested**. The real custodian and ordinary unlock path meet the intended epoch design, but the lease can be released while the host still serves.

## Finding

- **P2: Keep the root lease through the whole host lifetime.** `ports/djinn/custodian/src/fixture.rs:131-134` moves the only lease into the custodian listener thread and discards its join handle. `Service::serve` can return on a listener accept error or a session-thread spawn error (`ports/djinn/custodian/src/driver.rs:71-83`). On that path the lease is dropped, but the process's app broker at `ports/djinn/tests/custody_payload.rs:395-396` and previously started service threads can remain alive. A second caller of `fixture::receipt` can then acquire the root lock while the first host still exists; the broker endpoint check ordinarily prevents a second process host while its socket remains present, but it does not make the lease itself cover that lifetime. Retain the lease in an owner that lives for the host's full serving lifetime, or stop the whole host when the custodian server exits. Verify the failure path as well as normal restart.

## Scope and evidence

Reviewed Mere base `a4cbd4baf3411b597bdcfc13653de0444b5d480d`, the exact `m5-real-custody-review.diff` (SHA-256 `caecdbeffb1ee6b9518ca0638302e9dd032de3fad585b770ce57391ca0590e5a`), the brief and fixture report. The diff matches the live changes apart from blank separator lines. Live source hashes match `m5-real-custody-review-hashes.json`:

- `ports/djinn/custodian/src/fixture.rs`: `326677d8aa2792db74f0da01e757203a7d3504fa77601cd6d68b6c55e92cb62c`
- `ports/djinn/tests/custody_payload.rs`: `c6ee9d37aab9b37a637281ba76bb67f60ffac6fa49c4ad7a9171ca24f1c83eaf`
- `ports/djinn/tests/custody_receipt_tests/mod.rs`: `b23b1730a7782330111ef81e315766a274dba543d579ef845032e12c758671f7`

`Core::start` supplies a new instance and locked state; `RemoteHost::custody_epoch` obtains the valid generation and instance from status. The host uses the ordinary remote unlock, checks the synthetic profile and wallet on reopen, and grants only the custody app route. The root is required to be marked, private, owned and canonical; new roots reject other content and reopens require initialized state. The vault and wallet are created only on the new path, with missing or corrupt state rejected on reopen. Random runtime directories avoid reuse of custodian sockets, and the host refuses an existing app endpoint. No retained research data is seeded. The existing `fixture::start` follows the same create path, and the reviewed changes contain no Cargo or Turnstone admission change.

The parent reports verified final focused receipts: three fixture tests passed for restart, profile fencing and damaged state; the existing lock/unlock fence test also passed. Those tests do not cover the server-thread failure above. I did not rerun tests or build. Actual application use with this fixture and M5 offline-restart qualification remain separate and unverified here.
