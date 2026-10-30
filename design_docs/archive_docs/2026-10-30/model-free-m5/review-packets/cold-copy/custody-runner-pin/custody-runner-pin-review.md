# Custody runner pin review

Scoped verdict: approved. The diff changes only the runner pin in `verify.py` and verifier pin in `assemble.py`. The runner pin matches the reviewed runner SHA-256 `260d7e33b73d3abafea742007e150b3be46f12721f3669366d7e472989b046c5`; the assembler checks the current verifier SHA-256 `d0e181a75e6a1d4d142b3bc4e3aa50461259c25d24ba0bc87b1992a30b5280e2` before loading it. Assembler SHA-256 is `1eee7a19c7dff13c8b5e7245a8db81adda2410cb6446a1575c673a987e9b01b2`; diff SHA-256 is `9ed424e38aeecc067a867d149ad6562f50db7484c69a11638309c272530cc605`.

The recorded `custody-runner-pin-tests.log` reports 43 passing tests. Tests were not rerun for this scoped review. The fault guard remains false and final verification remains draft; this is not a runtime or acceptance approval.
