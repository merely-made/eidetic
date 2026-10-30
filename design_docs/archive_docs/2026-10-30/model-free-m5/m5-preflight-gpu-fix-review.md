# Empty-startup GPU discovery fix review

Verdict: **the narrow sysfs delta is suitable for another bounded preflight attempt**. No blocking mount-validation or receipt-claim defect was established in this review.

The runner derives the sysfs link from the validated render character device's major/minor, requires the resolved target to be the corresponding `drm/renderD...` node under `/sys/devices`, and read-only binds its enclosing device subtree rather than all of sysfs. It records the resolved subtree and target identities, checks them again before launch, and checks the mounted target and identities before the phase starts. Existing cgroup visibility remains separate. The added logging changes diagnostics only.

The supplied comparison reports Radeon discovery with the narrow mount; it does not demonstrate that Turnstone selects the adapter, renders, or presents a frame. Attempt C's retained cleanup receipt reports its scope cgroup empty; that attempt did not produce a successful frame. All six current inputs match the supplied hash manifest. The implementer reports nine passing helper tests and a command-construction check; neither was rerun here. No application or custody runtime was run for this review, and this verdict is not M5 acceptance.
