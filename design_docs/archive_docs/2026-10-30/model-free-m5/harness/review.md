# Provisional M5 harness archive review

Reviewed archive sibling layout, script pins, fixture reconstruction, manifest and source bytes, and documented test discovery. This is a review of the archive, not native qualification.

- All 40 listed files match their manifest SHA-256 and their available original source bytes; decoded synthetic audio also matches its recorded decoded hash and original bytes.
- Receipt integration's three pinned sibling modules and its pinned `verify.py` match the archived copies. The missing-source script's local companions and scenarios are present. Product source, build inputs, host paths and the guard installation remain external as described.
- Independent discovery in copied directories passed acceptance 4, draft verifier 3 and missing 17 tests; integration 42 was separately reported passing. No native launcher or build was run in this review.
- Documentation correction: `Python 3` is too broad: `Path.is_relative_to` requires Python 3.9 or newer. Other host capabilities still need actual validation.
- The copied historical integration README links `../m5-missing-draft/fault-ledger-fix1-review.md`, which is absent from that sibling location. The review is retained at `../review-packets/fault-ledger/fix1-current/fault-ledger-fix1-review.md` relative to the harness directory. The top-level README should identify this historical broken link and point readers to the retained copy.

No missing script dependency or byte mismatch found. Approval of archive layout is conditional on clarifying these documentation issues. The disabled missing driver, DRAFT final guard, and pending intact/fault and visual evidence remain qualification boundaries.

## Follow-up

Both documentation findings are resolved in the top-level harness README: it specifies Python 3.9 or newer and links the retained fault-ledger review while identifying the copied integration README as historical. The linked review exists. Archive layout approved within the scope above; native qualification remains pending. No tests or native launchers were run in this follow-up.
