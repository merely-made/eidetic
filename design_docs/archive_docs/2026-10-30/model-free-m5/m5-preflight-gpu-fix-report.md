# Empty-startup preflight GPU discovery delta

Attempt C is preserved at `/home/markik/Code/testing/m5-empty-preflight-20261030-c`. Its actual custody probe passed with `closed-during-open` and same-principal/live-peer checks. Turnstone started, but `scenario.done` recorded no successful presentation and zero captures; the exact process group/cgroup cleanup is verified. No frame or startup success exists.

The focused locked-resource diagnostics are comparative, not Turnstone presentation receipts:

- `m5-vulkan-nosys-1` used the Radeon ICD, the selected render node and the current cgroup-only sysfs view. It exited 1 with no valid Radeon GPU.
- `m5-vulkan-sys-1` varied by adding read-only sysfs and exited 0 with AMD RADV Renoir.
- `m5-vulkan-narrow-sys-1` used only read-only `/sys/devices/pci0000:00/0000:00:08.1/0000:06:00.0`, the corresponding `/sys/dev/char/226:128` symlink, and the existing cgroup view. It also exited 0 with RADV Renoir. Log SHA-256: `e5678eeea0bfd8cfe40e7fca44e8038cef5db1f5abb1846da51d92cd91500f76`.
- Separate `m5-font-no-config-1` and `m5-font-local-config-1` probes both resolved Adwaita Mono. The latter suppressed the missing-config warning, but font configuration remains unchanged in the proposed retry to avoid mixing variables.

The runner resolves the selected validated render device's existing `/sys/dev/char/major:minor` link, requires its canonical target under `/sys/devices` to end in that same `drm/renderD...` node, and read-only binds only its enclosing device subtree. The mount, source link and exact device/target inode identities are recorded and rechecked before startup. It does not bind all of `/sys`. The only environment change is the requested `RUST_LOG=warn,wgpu_core::instance=debug` for existing adapter diagnostics. The 5-second frame qualification deadline, network isolation, feature/build evidence and resource caps are unchanged. Whether Turnstone selects RADV or presents successfully remains untested.

`/usr/bin/python3 m5-draft/test_preflight_helpers.py` passed 9 tests in 0.122s, including target validation and existing cleanup/mount/PNG checks. All preflight Python files compile. A separate harmless command-construction check using actual host device identities confirmed the narrow expected PCI subtree and symlink appear in the constructed arguments and a broad `/sys` bind does not. No app or custody retry was run with this delta.

Frozen SHA-256: `run_preflight.py` 2b968f0e08d7473d4091b8ff382bfdcae9779cf065b0674c6bad2bc7486488d5; `preflight_phase.py` 48238cab03fc20a2925d83e99dc06e66d0c50c41967b838ad7a382130385b48f; `run_native.py` bf14ab5d772c8a713d6e0473642e30922af180359de7bdbc58b9a4b8279aa29f; `namespace_phase.py` f17afe791dfb7b3d02a63be5f00497ee59ddff45bfda6822e89d5c5445071121; `fixture_server.py` 71198b8d604d59a849b3c5c0218f7e26735cfae285b066cea88efb9c369becca; `preflight.scn` 961b17e4c0432d6b29b97bc47bec11722bcf906fe795146f7a12c2bb9000b8dc. The test file hash is `09fff59db698109cb638136a1defa6903f71ba5c8286ea714ffc84e7c486d897`.
