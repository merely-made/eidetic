# Real custody fixture build feature review packet

The existing `custody-receipt-host` feature enables the real custodian, Core, and RemoteHost integration added to the existing ignored `custody_payload` fixture process. The supplier change does not add or alter a Cargo feature. It is a fixture-only mode, not a claim that the resident package is absent. The other five reviewed preflight scripts are unchanged. Prior failed runs A-D are untouched.

`m5-draft/capture_build.py` now builds that fixture using precisely `cargo test --locked --offline -p mere-resident --features custody-receipt-host --test custody_payload --no-run`. `m5-draft/run_preflight.py` accepts only that exact custody command, rejecting additional flags including inference, embedding, GPU, and all-features. This checks explicit command arguments, not a complete dependency or runtime activity audit. Supplier source review, feature closure examination and runtime model-activity evidence remain separate requirements.

Review diff: `m5-preflight-real-custody-feature.diff` SHA-256 `07350759eaaf42ec9a1222835a1bfea2481775b15b7bdc67710c1caa03f5ad94`.

Frozen file hashes:

- `m5-draft/run_preflight.py`: `6a832775e4983d8fb72adfa5c926c92eb99faaec420ed01c0528b99532e8358d`
- `m5-draft/capture_build.py`: `12f3d031b3de09b3b2dd0a573e7b48d8e04b9b89aecd3fd316d1fc78a72cfbfa`
- `m5-draft/test_preflight_custody_command.py`: `2184e867ddbf558830f745df63580eab17bc4a8e7b4f06e7bbde66c945ea0355`

Covering check, run from this SDD directory: `python3 m5-draft/test_preflight_custody_command.py` produced `Ran 1 test ... OK`. It executes the actual launcher's extracted AST predicate for the exact command and rejects added feature aliases, all-features, a missing offline flag, and a substituted feature. `python3 -m py_compile m5-draft/run_preflight.py m5-draft/capture_build.py m5-draft/test_preflight_custody_command.py` exited 0. The fixture's first focused lifecycle test passed separately; its strengthened source is still under test and review. No new preflight has run.
