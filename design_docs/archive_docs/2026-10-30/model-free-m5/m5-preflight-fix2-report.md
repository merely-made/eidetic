# Empty-startup preflight review fixes, round 2

The systemd scope now has `RuntimeMaxSec=240s`, `TimeoutStopSec=3s`, `KillMode=control-group`, and an explicit `app.slice`. This is an independent deadline even if the controller is interrupted or cannot inspect the unit. A successfully created scope carries those properties from creation. Scoped signals still require the observed process-tree cgroup and matching invocation. On every launched exit, the runner checks the exact named cgroup's kernel `cgroup.events` for `populated 0` or the unit cgroup's absence; if ownership was never established, it waits through the backstop deadline before treating absence as meaningful. An unavailable or nonempty cgroup is recorded as unverified; a normal exit cannot succeed on that basis. Cleanup cannot signal an unverified unit.

The host's `user@UID.service/app.slice/<unit>.scope` path was verified with a separate harmless 2-second scope: it existed and reported `populated 1` during sleep, then the process exited -15. This is host-specific and remains a bounded verification, not an M5 app receipt.

A stronger harmless disposable scope probe used a Python child that ignored SIGTERM. Command `/usr/bin/python3 m5-draft/probe_scope_backstop.py` wrote `m5-draft/scope-backstop-probe.json`: `RuntimeMaxSec=1s`, `TimeoutStopSec=1s`, `KillMode=control-group` terminated the child with exit -9 in 2.112 seconds. The probe started no app, custodian, fixture service or acceptance scenario.

Pure helper tests from this directory:

```text
$ /usr/bin/python3 m5-draft/test_preflight_helpers.py
.......
----------------------------------------------------------------------
Ran 7 tests in 0.059s

OK
```

They include backstop property/bounds, waiting until the backstop when ownership was not observed, collision refusal, signal-failure escalation, unowned no-signal behavior, mount input rejection, and PNG validation. The actual namespace launcher and its ownership observation remain unrun. No preflight or acceptance result is claimed. Frozen hashes are recorded separately by the parent before re-review.
