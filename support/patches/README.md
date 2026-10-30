# Storage engine sources

These sources derive from published crates fjall 2.11.2 and lsm-tree 2.10.4,
both licensed MIT OR Apache-2.0. Upstream license files and notices are retained.

- fjall 2.11.2: https://github.com/fjall-rs/fjall; crates.io checksum
  `0b25ad44cd4360a0448a9b5a0a6f1c7a621101cca4578706d43c9a821418aebc`.
- lsm-tree 2.10.4: https://github.com/fjall-rs/lsm-tree; crates.io checksum
  `799399117a2bfb37660e08be33f470958babb98386b04185288d829df362ea15`.

Direct path edges let Git consumers receive these exact sources. The M4
bounded read and non-mutating recovery extension is not complete or qualified.
Registry publication of Muniment needs a separate dependency strategy.
The derived Fjall manifest uses the workspace's Rust 1.97.1 floor for native
file locking; its upstream 2.11.2 manifest declared Rust 1.76.0.
The derived Fjall makes the partition tree private and replaces snapshot Deref
with forwarding methods so iterator and snapshot capabilities retain their
cooperative owner lease. This is an intentional upstream API change.

Bounded standard-tree preparation omits the optional Bloom filter and loads
full indexes under admission limits. The bounded read path independently
rereads index and data blocks under the shared call/operation ledger rather
than using cache entries whose original work was not charged. This preserves
lookup behavior but can increase disk reads. The byte-backed optional Slice
feature remains unsupported by bounded recovery and reads pending an
allocation audit.

The current runtime receipts cover Linux with Rust 1.99.1. Linux and macOS
path accounting follows the inspected standard-library Unix implementation;
macOS has not been runtime tested. Windows short absolute paths use the
source-audited UTF-16 conversion; long or nonabsolute paths explicitly refuse
until their normalization allocations are audited. No Windows runtime result
is claimed. Descriptor and path capacity bounds depend on the inspected Rust
standard library and pinned collection implementations, and must be rechecked
when the toolchain or those implementations change; the Rust 1.97.1 manifest
floor alone does not establish qualification on every later compiler.

After a successful temporary-store open, cleanup waits for the last owner
capability, removes the store contents, and retains the root and
`.fjall-owner.lock` coordination inode. Advisory locking covers cooperating
updated owners; arbitrary deletion or modification by other processes is
outside that contract.
