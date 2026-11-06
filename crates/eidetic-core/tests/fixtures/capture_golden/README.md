# Capture golden fixtures

Bytes written by Turnstone's native capture core (`src/trail_memory/` at
`origin/main` cc9cc6d2, built against Eidetic rev 1fcdb359), frozen before that
core was promoted into `eidetic::capture` (memory-gates plan S6, D17). Existing
stored sessions must stay readable and writable byte-for-byte, so these are the
reference the promoted code is held to: `tests/capture_golden.rs` reads every
key below through the promoted code and, where the format is canonical,
re-writes it and compares bytes.

## Origin

No Turnstone M1-M5 session directory survives on this machine (searched
`/Users/markik/Code/testing`; nothing carries the `sealed-source-capture`
namespace), so these were produced by running Turnstone's own code, not
Eidetic's, in a scratch clone with a one-off dump test (the clone and the dump
test are not committed anywhere). The calls were Turnstone's
`sealed_capture::capture_request` and `acknowledge`,
`capture_source_document_in`, `sealed_records::deposit_definition` and
`deposit_record`, and Eidetic's `BrowsingMemory` and `PageTextStore` as
Turnstone drives them. The sealer is Turnstone's own test sealer (XOR `0x5a`
over the cleartext, hash-hex prefix, format `fixture`, epoch `[7; 16]`), the
store a `MemoryBackend`.

## Privacy

Everything is synthetic: the host is `fixture.example`, the page body is
"Synthetic capture fixture alpha beta gamma.", ids are counters
(`...1001`, `...2001`), and times are a fixed 2023 instant. No person, profile
or real browsing is in any file.

## Files

Each `*.kv.json` is a sorted array of `{"key", "hex"}`: a full dump of one
store, values as lowercase hex.

| file | family |
| --- | --- |
| `sealed_deposited.kv.json` | sealed namespace after `capture_request`: sealed raw+body blob (one, identical bytes), sealed Fleece annotation, their manifests with seal markers, the receipt envelope (`saved: false`, `stored` set) |
| `sealed_acknowledged.kv.json` | the same store after `acknowledge` (receipt `saved: true`) |
| `sealed_meta.json` | the request, stored identities and the document the capture used |
| `source_document.kv.json` | unsealed `capture_source_document_in`: raw blob and manifest (Imported provenance), annotation, Fleece schema blob and manifest, `resource-capture/` association |
| `source_document_meta.json` | the stored identities |
| `structured.kv.json` | sealed property definition and record (`deposit_definition`, `deposit_record`) plus the record schemas they bootstrap |
| `structured_meta.json` | the definition, record and their manifest ids |
| `browsing.kv.json` | a flushed `BrowsingTrace` segment and two `page-text/` records with their blobs and `resource-capture/` associations |

## Regenerating

Do not regenerate to make a test pass. A byte difference means the stored
format moved, which is a migration, not a fixture update.
