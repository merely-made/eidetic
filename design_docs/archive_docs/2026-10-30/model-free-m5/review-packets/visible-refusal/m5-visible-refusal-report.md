# Visible structured refusal

Collection P recorded the exact bounded limit refusal, but its frame still showed stale Home suggestions and no error. The next strict suggestion selection failed because a keyboard move recomputed the rows and inserted the previously missing error row during selection. P's owned seed cgroup was verified empty; it did not reopen.

`structured_error` now resets the suggestion window and recomputes it immediately when the Saved Sources structured view is open. It does not modify the matching request, scope, error, or limit-refusal evidence. The regression dispatches a real matching response while the window starts at a nonzero row, requires the exact `Unavailable: Structured record limit exceeded` row immediately and checks the existing request/scope binding and invalidation behavior.

Source diff `m5-visible-refusal.diff` SHA-256 `1cd7019c6cb8077917ac2aa9d196437e8e07116c0d75626c9a1a574b31f1d0a3`; owner source SHA `3ae7bde2c68a006f8b70446d4044f5297924d28833a5356f6805d2610660f1ee`; test source SHA `6f6f6201dfb32f7389e2e377e97947b43212e1a7ed578a37de84b56e85544a20`.

`m5-refusal-visible-red-1` failed but used the fixture's default Address lane, so it does not establish the real stale structured window. Red-2 set the actual Saved Sources lane and failed with stale structured rows while the refusal was held, SHA-256 `7b606bfe6fe0d00d621e440e6a621cd5796710562badec898aa879ab57fa8c6d`. Green-1 passed 1/0, SHA-256 `4b01d8d9ca23981d6cbcc634ed241a2fd040affddd74c792e792ca238ddd5849`. Source-size and diff checks pass. The source is frozen for focused review; the real refusal, settings recovery, and no-match visibility require a fresh native diagnostic.
