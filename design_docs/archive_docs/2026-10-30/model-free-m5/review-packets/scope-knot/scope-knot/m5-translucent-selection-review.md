# Translucent selection review

Approved as a bounded, source-only highlight change. At Turnstone HEAD `180df8758ba7c3be3d796287e1fcc0464a59dc9d`, the only dirty change replaces the selection background with valid CSS `rgba(42, 76, 118, 0.35)` and preserves the foreground. It changes styling only; source bytes and selection logic are untouched.

Reviewed file SHA-256: `6e22fcf77983e40180359253d7ffe0e9b20b05a7a20af166a1c6cb6f2f188e3e`  `src/knot_authoring/knot_sheet.rs`.

This does not establish the renderer's paint ordering or prove glyph readability. A fresh native rendering check is required before calling the display issue fixed. No tests or builds were run for this review.
