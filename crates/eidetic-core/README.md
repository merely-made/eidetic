# eidetic

Package and library are both `eidetic`. It was published as `mere-eidetic`
through 0.0.2; that name is frozen.

The owner-scoped durable-content library used by Mere and other hosts. It owns
the durable typed vocabulary: blob manifests, typed payloads,
schemas, codicils, bundles, packs, encrypt-at-rest markers, browsing traces, and
content-addressed model artifacts. Portable intake retains original source bytes
and reports conversion losses; content caching, image bytes and facet-schema
validation also live here. Backends and application-session authority stay with
Muniment and Pandect respectively.

## Modules

| Module | Contents |
|---|---|
| (root) | `Request` / `Response` (blob load/save), `Error`, `Result`, `dispatch`, `bootstrap` |
| `schema` | `Hash` (multihash-aware, `HashFn::Blake3` today), `HashError`, `ManifestId`, `SchemaRef`, `Timestamp`, `PrivacyClass`, `ProvenanceOrigin`, `ProvenanceRecord`, `TrustLevel`, `TrustEnvelope`, `SignatureRef`, `ModerationState` |
| `manifest` | `BlobManifest`, `BlobSource` (`Local`, `Embedded`, `Https`, `Iroh`, `LocalFile`, `LocalOnlyRef`), `BlobFetcher`, `NoFetcher`, `save_manifest`, `load_manifest`, `list_manifests`, `delete_manifest`, `resolve_blob` |
| `typed` | `TypedPayload`, `save_typed`, `load_typed`, `list_typed`, `save_typed_sealed`, `load_typed_sealed` |
| `schema_def` | `SchemaDefinition`, `SchemaFormat`, `SchemaValidator`, `MereNativeSchemaBuilder`, `MereNativeSchemaBody`, `MereNativeFieldSpec`, `MereNativeValidator`, `JsonSchemaValidator`, `JsonLdValidator`, `bootstrap_meta_schema`, `save_schema`, `load_schema`, `find_schema_by_id`, `validate_payload`, `validate_against_schema` |
| `codicil` | `Codicil`, `TimeBounds` |
| `bundle` | `Bundle`, `BundleMember`, `save_bundle`, `load_bundle`, `verify_required_members` |
| `pack` | `PackManifest`, `PackPart`, `PackPartRole`, `PackVerdict`, `canonical_bytes`, `sign_pack`, `verify_pack` |
| `seal` | `PayloadSealer`, `SealEpochId`, `SealedBlobRef`, `seal_marker`, `seal_payload_for_store`, `resolve_sealed_blob`, `is_private_lane` |
| `browsing` | `BrowsingTrace`, `BrowsingMemory`, `TraceEvent`, `TraceTransition`, `PageRef`, `save_trace`, `bootstrap_browsing_schema`; `browsing::lineage::project_lineage` behind the `lineage` feature |
| `import` | Portable browser interchange, `Intake<T>`, original source bytes, provenance and conversion notices |
| `content_store` / `image_content` | Store-neutral fetched-content cache and content-addressed image bytes, with stable existing keys |
| `schema_facets` | Eidetic schema registration and Chartulary facet validation |
| `deleted` | `DeletedNode`, `record_deleted`, `list_deleted`, `purge_deleted`, `clear_deleted` |
| `models` | `ModelManifest`, `ModelLibrary`, `ModelComponents`, `OpaqueBlob`; `models::embedding` binds an exact embedding space, chunker and source revisions for compatible derived indexes |

## The store seam

`eidetic::Store` is a re-export alias for [`muniment::Backend`]. `Backend`,
`MemoryBackend`, and `WriteOp` are re-exported alongside it, and
`muniment::error::StoreError` converts into `eidetic::Error` through a `From`
impl. Any muniment backend is an eidetic store.

The trait is `async` and `?Send`: a browser store awaits JS promises, native
stores return ready futures.

## Features

| Feature | Default | Effect |
|---|---|---|
| `pack-signing` | yes | Pack signing and verification via `personae` (`dep:identity`) |
| `json-schema` | yes | Full JSON Schema validation via `jsonschema`. Without it, stored JSON Schema definitions return a "validator unavailable" error |
| `lineage` | no | `browsing::lineage`, which projects a `chartulary::stemma` into `BrowsingTrace` codicils |

## Explicit retained copies

`retention::retain` stores one verified, owner-scoped payload, local manifest and
copy receipt through a single Muniment transaction. Callers supply copy permission,
payload/stored-byte ceilings, policy revision, clock and wallet sealer. Repeating
the same copy preserves its receipt and does not charge it twice. Stored-byte
quota includes encryption overhead; metadata and other namespaces are outside it.
Backend reads still materialize values before verification and size checks.

Private copies require a `PayloadSealer`; keyless plaintext fallback is refused.
`retention::read` verifies the actual held bytes, identity and seal metadata rather
than inferring custody from a receipt. Redb supports the required transaction;
the current Fjall backend refuses this operation. Retaining is separate from
URL caches, public export, hosting, expiry and permission to erase.

## Dependencies

`muniment` (path), `chartulary`, `async-trait`, `blake3`, `serde`, `serde_json`,
`url`. Optional: `personae` (as `identity`), `jsonschema`.

## Companions

- `muniment::FjallStore` (Muniment feature `fjall`): the native LSM backend.
  Existing stores keep their `eidetic` partition and opaque keys.
- `fetch::https_blob::HttpsFetcher` and `fetch::iroh_blob::IrohFetcher`
  (Mere Fetch features `https-blobs` and `iroh-blobs`): optional native
  implementations of Eidetic's `BlobFetcher` contract. `resolve_blob` retains
  content-hash verification and local caching; Eidetic has no concrete network
  or database implementation in its runtime dependency cone.
- `eidetic-search`: derived lexical recall in Mere's
  `crates/intel/eidetic-search`. Its production library uses in-tree weighted
  BM25, Genet text segmentation and Eidetic records; the caller supplies vector
  rankings to its fusion seam. ESP is an example/dev integration, not a
  production dependency of the lexical library. Indexes are rebuildable views
  over retained evidence.

## License

MPL-2.0 (see LICENSE).

[`muniment::Backend`]: https://docs.rs/muniment/latest/muniment/trait.Backend.html
