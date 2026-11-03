# line-ceiling rollout receipts

## 2026-11-02: E4 and Linux E2

Before E4: eidetic `6d7b73e`, line-ceiling and responsa tags v0.0.2.

- Windows, prek 0.5.9: `prek install --allow-missing-config` installed
  `.git/hooks/pre-commit`; `prek run --all-files` passed.
- Exact tracked inventory: 20 owned oversize files match the 20 exception
  paths in `line-ceiling.toml`; 10 oversize imported files fall under the
  single `support/patches/` exclusion. Every entry has a nonempty reason.
- A staged `.house-style-control.rs` with 601 physical lines failed
  `prek run --all-files` (exit 1), then an actual `git commit` through the
  installed hook (exit 1). Both reported only that control file. HEAD was
  unchanged. The control was unstaged and removed; the full run passed again.
- T-PC: SSH succeeded; `hostname` returned T-PC, `uname -s` Linux;
  `avahi-resolve-host-name T-PC.local` returned the host's expected address.
  rustc 1.99.1. The pinned upstream prek 0.5.9 GNU Linux release archive
  passed its published SHA-256 check and installed to `~/.local/bin/prek`.
- Linux tagged install hooks used `language: rust` and the README's
  `cli:https://github.com/merely-made/eidetic:<tag>:<package>` dependencies.
  `responsa-v0.0.2` installed and invoked `responsa lookup stack.7` in the
  existing Mere checkout in 37 seconds. That older checkout reported
  `stack.7: neither declared nor cited`; this is installation/invocation
  evidence, not qualification of its old manifest.
- `line-ceiling-v0.0.2` installed and passed against the existing Eidetic
  `crates/proofs/src/lib.rs` in 9 seconds. An earlier attempt selected no
  tracked files and was skipped; it is not counted as an install pass.
- Remote source checkouts were not updated. Temporary hook configuration,
  release archive and checksum files were removed. Ordinary prek tool
  environments and the installed prek executable remain for reuse.

Retained evidence is this compact text receipt, under the 100 MB lane cap.

### Deletion-only follow-up

The shared stanza and both adopter configs set `always_run: true`.
In eidetic, a temporary exception for the nonexistent
`.house-style-deletion-control.rs` failed
`prek run line-ceiling --files .house-style-deletion-control.rs` with exit 1
and `stale exception ...: gone`. prek discarded the nonexistent filename,
so the tool received an empty file list. After restoring the exact original
configuration bytes, the same command passed rather than being skipped;
the full-tree run passed too. No control source file was created.
