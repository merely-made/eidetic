# M5 Redshank note controls

Status: the supplier and host adapter passed focused gates. The second host review identified same-episode reopen draft loss; its correction passed the combined Redshank gate and is frozen for scoped review. This is not a headed or product-use receipt.

Turnstone checkout: `0089c9d331522343d81a69dcf1f3650ec662f4a4`. The initial source investigation used Woodshed `a4356b74084e9016f6fdb9a7c6b8130def547021` and Mere `c1035649255c934e057fe7eb0e0ebdf8881c1024`; the parent subsequently received the reviewed supplier cohort before the final host test.

The pinned Redshank surfaces expose `tabs::listen::panel`, `RedshankSurfaceState`, `TextCapture`, `FullView` and `surface`. The panel uses the existing text editor and note privacy row. The compact erased session only exposes the compact dock, so a host session adapter would need to bridge the full state and commands while preserving the local editor across projections. Supplier changes are staged in an isolated worktree; no Turnstone lock changes have been made.

The staged host test `admitted_episode_exposes_its_note_list_alongside_the_dock` failed as expected because the admitted dock did not contain the note list (`/home/markik/Code/testing/custody-contract-20261029/m5-note-host-ui-red-1.log`). It remains red until the adapter is implemented and the reviewed supplier is received.

The host test `same_offset_captures_have_distinct_generations` failed at the expected missing generation hook after supplier reception (`m5-note-generation-red-1.log`). The host wave adds a distinct generation to each frozen capture and invalidates it on save, cancel or selection change. It also introduces a retained bridge around the existing Woodshed listen panel and compact dock, preserving local draft state across projection refreshes, and rechecks rendered item, session, source-capture generation, custody epoch and capture generation before applying note writes. The first combined `authoring,inference` gate failed at compilation with E0451 because a struct-update initializer attempted to access the supplier's private command queue (`m5-redshank-combined-1.log`, SHA-256 `509b44d3b9da9a43b0e9ba5c495f73575e16530ec718e44af162abe7064be52e`). After changing the host constructor to use the public default, the second combined gate passed 29 tests with one ignored (`m5-redshank-combined-2.log`, SHA-256 `8af3406230e32597755164a904cc6d9b52d6c5f818304ef772e9f02485e95366`). The subsequent source split and new event test require the running gate. No headed runtime result is claimed.

The stock compact text-note button is disabled when transport is buffering or unavailable. Turnstone's silent host can nevertheless freeze the saved position, including 0 ms. A no-playback path needs an explicitly available capture affordance; it must not represent unavailable transport as playing or paused.

The approved supplier addition is an optional explicit text-capture availability on `CompactPlayerState`: absence retains the existing transport-derived policy; Turnstone will supply the current item, custody and capture capability. This keeps the existing button as the single capture affordance.

The original host accepted a save after cancellation, or silently substituted a frozen target for a mismatched supplied anchor. The focused `text_note_save_requires_the_current_frozen_anchor` test was run through the build coordinator and failed as expected at the first refusal assertion (exit 101; `/home/markik/Code/testing/custody-contract-20261029/m5-note-anchor-red-1.log`). The host has since been changed to require the selected item and matching pending anchor, and retain that anchor after a rejected save. The same focused test passed under the authoring build, 1 passed (`/home/markik/Code/testing/custody-contract-20261029/m5-note-anchor-green-1.log`). The adjusted graph/reopen test also passed, 1 passed (`/home/markik/Code/testing/custody-contract-20261029/m5-note-reopen-1.log`). A later cleanup removed unused imports and rearranged the successful-save block without intended behavior change; those exact final sources have not yet been retested. Equality of targets alone is insufficient: two captures at the same position can have equal anchors. An integration must bind a distinct capture generation as well as item, session and custody epoch.

The supplied note row always exposes Edit and Delete as well as privacy. Turnstone currently ignores BeginEditNote and its Delete path does not reconcile the graph projection. The pinned row offers no capability filter for these actions. The staged supplier cut adds a `NoteRowActions` policy in `RedshankSurfaceState`, defaulting to the existing full set and offering `PrivacyOnly` to omit Edit and Delete while retaining the existing privacy control and editor. Turnstone will choose `PrivacyOnly`; the standalone host retains the current default.

Read-only supplier check: `origin/main` was `b790b3c1f9c4a08a599e5ad746c39134ee22810a` when checked with `git ls-remote`; pinned `a4356b7` is its ancestor, and no commits in that interval changed `ports/redshank/surfaces`. The clean detached `woodshed-djinn-ownership-20261030` worktree is at that tip. The main `repos/woodshed` checkout is clean and behind upstream; existing worktrees were left intact.

The parent approved both narrow supplier extensions and created `woodshed-model-free-notes-20261030` at the fetched tip for them. The supplier visibility test failed as expected at the currently visible Edit action (`/home/markik/Code/testing/custody-contract-20261029/m5-note-ui-red-2.log`); the preceding root-workspace attempt failed to select the nested package and is not a test receipt. The explicit text-capture availability test likewise failed at the expected missing AddTextNote command (`m5-note-ui-red-3.log`). Implementation is staged in `ports/redshank/surfaces/src/lib.rs`, `dock.rs`, `tabs/rows.rs` and `tabs/listen.rs`: full note actions and transport-derived capture stay the default, the privacy-only row hides Edit and Delete, and an explicit capture availability applies only when an item exists. The nested surface library gate passed 111 tests, including both changes and the existing full-action test (`m5-note-ui-green-1.log`, SHA-256 `172849bf7bf2dd9067a8ff2585bb49ae2a2ab3bca0fb62f3896af9500c4a22b3`). The frozen supplier diff has SHA-256 `77825b91cdb87903b03c2e276713d6c1b546eee5371f26a9a285edf457bbcd52`; publication and Turnstone reception remain parent-owned.

The parent published Woodshed `b3dfacff93fcf0169097606589a6482b209ed047` and received its source substitutions in Turnstone's lock. Locked offline metadata passed; this does not by itself validate the adapter or product flow.

The host source was split to satisfy the repository's 600-line ceiling: `src/app/redshank_notes.rs`, `src/app/redshank_notes_tests.rs`, `src/redshank_notes_session.rs`, `src/redshank_episode_surface_event_tests.rs`, and `src/redshank_host/note_capture_tests.rs` are focused additions, beside the existing Redshank host, episode surface, app arms, and shell pump changes. `python3 scripts/check_source_size.py` passed after the split and `git diff --check` passed. The post-split gate ran `cargo test --locked --offline -p turnstone --features authoring,inference --lib redshank -- --nocapture`: 30 passed, 0 failed, 1 ignored. The log is `/home/markik/Code/testing/custody-contract-20261029/m5-redshank-combined-3.log`, SHA-256 `a02f5af695919d0e821e63de4c0e0e720fb32621eeeadc35dd1d9181f244ca51`. The retained-session test drives the actual Add, typing, refresh, Save, note menu and privacy events using a unit-test projection. The scoped app test separately checks rejected stale writes, a successful text save, private export refusal, privacy transition, selected owner export, and custody loss. These tests do not establish sealed retention or a headed product flow.

The consolidated host review (`m5-note-host-review.md`) found that Replay and other transport actions could bypass rendered-owner checks, including a stale Play selecting another episode and invalidating its newer capture. Both new regressions failed against the reviewed source at the expected admissions (`m5-redshank-stale-red-1.log`, SHA-256 `ffb77199cf99d6f18074ef079db803c82c8667bd4e7d35f07a52d938c1af56c4`). The correction binds all admitted actions to session, source generation, optional custody epoch, item, current selection and a host control generation advanced on selection and capture transitions. Replay and other model writes require live custody; fresh non-writing transport remains available without it. Stale tests include selection and capture round trips, a queued A control against B's typed draft, and an actual saved A note for the stale OpenNote cases. The first correction gate failed before tests because `control_generation` was mistakenly added to `PendingNoteOpen` rather than `RedshankHost` (`m5-redshank-stale-green-1.log`, SHA-256 `69163e839b03ae508147bd4cd0086d4a0f5a0ceec06992fef848b69a7063b8fc`). After moving the field and strengthening the OpenNote regression, the combined gate passed 33 tests with one ignored: `cargo test --locked --offline -p turnstone --features authoring,inference --lib redshank -- --nocapture` (`m5-redshank-stale-green-2.log`, SHA-256 `dc6b3307baeb23846b6c5c12e2f93c2421397c58f5e1f3b47da8e073eb20bd12`). These are silent-host and retained-session software tests; active audio-device behavior remains untested.

Pre-review host source hashes (SHA-256; superseded by the correction wave):

| File | Hash |
| --- | --- |
| `src/app/redshank_arms.rs` | `257fedc6f2eb535458411e0de177dadb0437e20e639ff418f968b75f1fee40b6` |
| `src/app/redshank_notes.rs` | `ea28af6e7c10b016d415bd4b06d7f86955ec655004da35331c15de2c1979b30e` |
| `src/app/redshank_notes_tests.rs` | `4c1ad8434c9cfec294608e2ae7a464eb1e9e9f39ffb682f03e063e5877ae2fef` |
| `src/redshank_episode_surface.rs` | `8fd747b62125cd1e7e2ca48ca0f012739756e45b1387935b043f04896e9ea106` |
| `src/redshank_notes_session.rs` | `4c6fafc1dc61af511b1d1bc93f4251ce9dea80805fbaa56653a90b92f5a0b0d2` |
| `src/redshank_episode_surface_event_tests.rs` | `4c99f00fba10c9e49bdf5ef459f6768b3b3f60ee43f146f0ccab49359ebf59ef` |
| `src/redshank_host.rs` | `c7607575ccdf2c7dafc5672ff682066d4fe8dd597b54ac9a30f4ad7e548d0558` |
| `src/redshank_host/impl_default_for_redshank_host.rs` | `1b32cad4b2a421a4ff6f9a52de0c09eb6bda057269d4ca0c957789f033b233a4` |
| `src/redshank_host/redshank_host_command.rs` | `27a13330a414ac3ccfb6b7055e0e285ad8ccd4247c597f817b8c206c12183858` |
| `src/redshank_host/redshank_host_retained_voice_preview.rs` | `4ad886d8d33e207cb6380c6505d114aee1862e82c7097d7428e45d9f9c096965` |
| `src/redshank_host/tests.rs` | `ca94304578940277d886199404be8b9e5e7a227799d2c8fa27e0b7bdd5b5005c` |
| `src/redshank_host/note_capture_tests.rs` | `fe4f3a749be699f0ca0ae41a597fba540bda0df318199c7c7253e8689d1c54fa` |
| `src/shell/events/shell_drain_actor_updates.rs` | `3d9b49ce9967d55703bfb9ce1cffde3021501430e9da71f957e3e11f914b91a8` |

First correction-wave source hashes (SHA-256; superseded by the reopen correction):

| File | Hash |
| --- | --- |
| `src/app/redshank_arms.rs` | `257fedc6f2eb535458411e0de177dadb0437e20e639ff418f968b75f1fee40b6` |
| `src/app/redshank_notes.rs` | `10d3d776871df7d2230dca35b1086a1c20388e0c09614033b78254b0da34c3f6` |
| `src/app/redshank_notes_tests.rs` | `6b68313f53296695be47adff76131882e4cf36c5f7ea70df0e4aaec5899f8565` |
| `src/app/redshank_transport_tests.rs` | `4bfb8dc2e1dea99b94f3b9cd75f626e2fa19b10d06bcb2940c3512aafe6a1f06` |
| `src/redshank_episode_surface.rs` | `49b572a7c40b8fa61e15f347e19da1a62fe8b14406fc178698ad8acabab0f5fe` |
| `src/redshank_notes_session.rs` | `adff44b9f7c0948111d13981f5a4aafc095d3fc2f13f5b2e94327e3e345ccb17` |
| `src/redshank_episode_surface_event_tests.rs` | `1c721e4f8943e112eabcab38e07f4c084186c9e4ce0ffa124b6990ee24ad637c` |
| `src/redshank_stale_transport_event_tests.rs` | `d9226cfe5ab103ff8a722cbea87034db17120dcd68f130bd3ddd935410b3b257` |
| `src/redshank_host.rs` | `dd58ff3961d366c0c2a358ba771475cb3b9f32a7d68867e727fd39ecbe2371dc` |
| `src/redshank_host/impl_default_for_redshank_host.rs` | `777948d2f047376264d6ff361af00250f123c8e97535bc71d12abd69898559a9` |
| `src/redshank_host/redshank_host_command.rs` | `d733774bd4dfe48960d3d5aab6b052af5f2de146eb59a0fb82f4e5a6bed8d9ef` |
| `src/redshank_host/redshank_host_retained_voice_preview.rs` | `c12f6fa9670290c366475a62b4f81cbbb5cd0fc306db4fbfeaa5f1df130f61a9` |
| `src/redshank_host/tests.rs` | `ca94304578940277d886199404be8b9e5e7a227799d2c8fa27e0b7bdd5b5005c` |
| `src/redshank_host/note_capture_tests.rs` | `fe4f3a749be699f0ca0ae41a597fba540bda0df318199c7c7253e8689d1c54fa` |
| `src/shell/events/shell_drain_actor_updates.rs` | `efeee2b40210539dbd3107b3ff844e15600e2d4f665fcd3e53b96b324e5b8093` |

No real fixture, headed UI, private refusal, retention, reopened process, keyboard, accessibility or viewport qualification has been run in this slice.

The scoped fix-1 review (`m5-note-host-fix1-review.md`) found that reopening the currently selected episode advances the control generation but retains its capture. The shell then mistook that generation change for capture invalidation, and the retained session also cleared the editor on any scope change. The new retained-session regression types a draft, queues a Play control, calls the ordinary episode reopen path, runs the shell's shared capture reconciliation, rejects the stale control, and projects and saves the original text with the same capture generation. Before the fix, `cargo test --locked --offline -p turnstone --features authoring,inference --lib redshank_reopen_preserves_retained_draft_but_rejects_old_controls -- --nocapture` failed because reconciliation canceled that capture (`m5-redshank-reopen-red-1.log`, SHA-256 `3422c6da37cb62b78cb485134cdfd8a09f0b46744b4dc12ffd2a2a323ebb6e93`).

The correction compares capture ownership separately from control generation during reconciliation and projection; it retains session, source generation, custody epoch, item, selection and capture generation as capture-owner boundaries. Full control scope still gates commands, and changed control scope closes menus and editing state. The coordinator ran `cargo test --locked --offline -p turnstone --features authoring,inference --lib redshank -- --nocapture`: 34 passed, 0 failed, 1 ignored (`m5-redshank-reopen-green-1.log`, SHA-256 `3b6e6901e7f11bc1d61cb19937909a093aaf9c353eae25f06b39270e935dfb30`). The gate used the existing shared lock, 3 GiB memory cap, zero swap, 150% CPU, and locked/offline dependencies. `python3 scripts/check_source_size.py` and `git diff --check` passed. No active audio-device, headed, retention, accessibility or viewport qualification follows from this software test.

Final host source hashes (SHA-256):

| File | Hash |
| --- | --- |
| `src/app/redshank_arms.rs` | `257fedc6f2eb535458411e0de177dadb0437e20e639ff418f968b75f1fee40b6` |
| `src/app/redshank_notes.rs` | `10d3d776871df7d2230dca35b1086a1c20388e0c09614033b78254b0da34c3f6` |
| `src/app/redshank_notes_tests.rs` | `6b68313f53296695be47adff76131882e4cf36c5f7ea70df0e4aaec5899f8565` |
| `src/app/redshank_transport_tests.rs` | `4bfb8dc2e1dea99b94f3b9cd75f626e2fa19b10d06bcb2940c3512aafe6a1f06` |
| `src/redshank_episode_surface.rs` | `54bc9f2008f0728ef8874a723864ba0e7373227a768e4f25d538fad75c6aeb98` |
| `src/redshank_notes_session.rs` | `ac5ab789ab4c3f28c145d19f18fd8bf2172e21e80111cd2b5789ee71f390b38d` |
| `src/redshank_episode_surface_event_tests.rs` | `1c721e4f8943e112eabcab38e07f4c084186c9e4ce0ffa124b6990ee24ad637c` |
| `src/redshank_stale_transport_event_tests.rs` | `d9226cfe5ab103ff8a722cbea87034db17120dcd68f130bd3ddd935410b3b257` |
| `src/redshank_reopen_event_tests.rs` | `e464d28add081655c85d6469da6203fc425b9febe653546a2c4638e52142ae24` |
| `src/redshank_host.rs` | `dd58ff3961d366c0c2a358ba771475cb3b9f32a7d68867e727fd39ecbe2371dc` |
| `src/redshank_host/impl_default_for_redshank_host.rs` | `777948d2f047376264d6ff361af00250f123c8e97535bc71d12abd69898559a9` |
| `src/redshank_host/redshank_host_command.rs` | `d733774bd4dfe48960d3d5aab6b052af5f2de146eb59a0fb82f4e5a6bed8d9ef` |
| `src/redshank_host/redshank_host_retained_voice_preview.rs` | `c12f6fa9670290c366475a62b4f81cbbb5cd0fc306db4fbfeaa5f1df130f61a9` |
| `src/redshank_host/tests.rs` | `ca94304578940277d886199404be8b9e5e7a227799d2c8fa27e0b7bdd5b5005c` |
| `src/redshank_host/note_capture_tests.rs` | `fe4f3a749be699f0ca0ae41a597fba540bda0df318199c7c7253e8689d1c54fa` |
| `src/shell/events/shell_drain_actor_updates.rs` | `28c2ba6b2e2816c85cbe05d5af7f7385a309d4ab8c0293c099b9a1269343d213` |
