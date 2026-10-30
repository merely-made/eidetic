# Visible refusal review

No critical or important findings in the scoped change. Approved for the planned native diagnostic.

Initially reviewed on Turnstone HEAD `d0d93da323249c001bd0ff214f69ee1885146ee9`; after upstream advanced, verified on HEAD `5b7dacd9a4777dbd68429db7ad7e9c174f468030` that the two-file live diff matches packet SHA-256 `1cd7019c6cb8077917ac2aa9d196437e8e07116c0d75626c9a1a574b31f1d0a3`. Exact files: `src/app/research_record_owner.rs` SHA-256 `3ae7bde2c68a006f8b70446d4044f5297924d28833a5356f6805d2610660f1ee`; `src/app/research_record_owner_tests.rs` SHA-256 `6f6f6201dfb32f7389e2e377e97947b43212e1a7ed578a37de84b56e85544a20`.

The error path clears busy and pending, sets the error, then resets and rebuilds the visible Saved Sources structured suggestion window. The existing query completion records the matching refusal after that rebuild, and its scope/request binding is unchanged. Other lanes are not rebuilt by this change. The regression exercises the actual lane and a nonzero starting row, checks immediate refusal visibility and retains stale-response and scope invalidation checks.

Review only; no builds were run. The focused green receipt in the packet is bounded. Native refusal visibility, settings recovery and no-match navigation remain to be checked.
