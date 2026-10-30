# Retained reader verifier repair

Turnstone `src/research.rs::clip_document` adds a selected heading and quote before Fleece's body. The prior verifier required the body at block 1 and allowed unchecked following blocks. The verifier now compares the complete expected title, optional selected heading and exact quote, body, owner-bound anchor metadata, documented recording badge and export link. It retains existing target, hash, selector, owner, session and source checks. The fixture follows the Q selection and note block shapes and exercises changed quote, body, title, metadata, export, extra prose and stale source.

`python3 -m unittest test_receipt`: 43 passed. Both Q reader documents were separately accepted using reconstructed owner and search records from their observed target and block data; this is a shape check, not independent owner or native acceptance. The DRAFT guard remains in place. Reader observations do not establish the active pane; separate visual frame review remains required. No native build, fault run, or final assembly was performed.

SHA-256 after change:
- `verify.py`: `4b48074459f282b90e184abe9ceddd237bc52a67c7650ff884d79a6f8cfd7383`
- `test_receipt.py`: `fd9af2661c9c78cddfd52a2d42612c2e29eae8afc4cb19cad441cb129882ada6`
- `assemble.py`: `98297755ce744c42fbec1a0b768e7e9130c954be0494403a657b354715c07c2c`

Exact patch: `m5-retained-verifier-report.diff`.
