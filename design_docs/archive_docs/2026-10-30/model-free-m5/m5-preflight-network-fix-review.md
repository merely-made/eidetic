# Empty-startup preflight network fix review

Verdict: **the reviewed delta is suitable for another bounded preflight attempt**. No blocking safety or correctness defect was established in this change.

The launcher creates fresh user and network namespaces, maps namespace UID/GID 0, and grants the setup process `CAP_NET_ADMIN`. The supervisor uses it for the loopback setup command, then sets `no_new_privs`, clears capability sets, and verifies effective, permitted, inheritable and ambient sets before starting fixture, custody, probe, or application children. Network-interface and cgroup checks remain before those children. The supplied isolated probe exercised the revised drop function, loopback, display and render-device access; its exec'd child reported zero effective and permitted capabilities and `NoNewPrivs=1`. The probe does not establish app startup or rendering.

All six reviewed files match the supplied network-fix manifest. The earlier preflight's retained cleanup receipt reports its scope cgroup empty; that attempt failed before custody or app startup and produced no frame. The report records eight passing helper tests. No probe, test, build, or preflight was rerun for this review, and this verdict is not M5 acceptance.
