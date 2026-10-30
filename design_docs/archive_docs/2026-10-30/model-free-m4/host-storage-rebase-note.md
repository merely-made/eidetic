# Turnstone storage draft reconciliation

Fetched origin/main 7ca35b714674cb367b549fccedc81c21210247bb. Rebased the three tested checkpoints without conflicts onto it; resulting HEAD 3113ef7f. Stashed the entire dirty draft including untracked files as 0d48efe036648a42cd835585e6a8a8111df933d9 and applied it without dropping it. Earlier stashes 614a8c063766af2c93fef9f6828975ab13c8add5 and aba47166ae23b73aac58fc18aa010eeee7dc38b5 remain.

The before and after inventories identify the same 39 tracked and 7 untracked files; 45 files match SHA256, status and mode exactly. The sole differing file is src/shell/shell_new.rs. Inspection of both diffs confirms upstream changes EventLoopProxy to ShellEvent, replaces unit wakes with ActorWake, and introduces NativeFrameReady and native frame state; the restored draft independently inserts configured storage limits before trail spawn and passes them to spawn_trail_for_session_with_limits. Both sets are present. No manual conflict resolution was necessary.

Lock retains upstream compositor-only Mere c629014 and 61-package cohort 229f277 from the local checkpoints; c629014 is an ancestor of 229f277. Eidetic a90aa678 is an ancestor of the local a025008c. Final supplier pins and lock verification remain pending. git diff --check passed. No build, additional commit or push was performed.
