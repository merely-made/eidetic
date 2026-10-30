use super::*;
use crate::manifest::{BlobFetcher, BlobSource};
use crate::schema::{
    ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, Timestamp, TrustEnvelope,
    TrustLevel,
};
use crate::schema_def::{
    SchemaDefinition, SchemaFormat, bootstrap_meta_schema, load_schema, validate_payload,
};
use crate::typed::save_typed;
use crate::{Error, Store};
use serde_json::json;

fn schema(id: &str, required: &[&str], fields: &[(&str, &str)]) -> SchemaDefinition {
    let mut specifications = serde_json::Map::new();
    for (name, kind) in fields {
        specifications.insert((*name).into(), json!({"type": kind}));
    }
    SchemaDefinition {
        format: SchemaFormat::MereNative,
        schema_id: id.into(),
        body: json!({"version": 1, "description": "Portable structured research record; intrinsic validation is also required.", "required": required, "fields": specifications}),
    }
}
pub fn property_definition_schema() -> SchemaDefinition {
    schema(
        "eidetic.PropertyDefinition/v1",
        &[
            "id",
            "revision",
            "value_type",
            "unit",
            "cardinality",
            "statuses",
        ],
        &[
            ("id", "string"),
            ("revision", "string"),
            ("statuses", "array"),
        ],
    )
}
pub fn property_record_schema() -> SchemaDefinition {
    schema(
        "eidetic.PropertyRecord/v1",
        &[
            "id",
            "entity",
            "property",
            "schema_revision",
            "definition_manifest",
            "context",
            "value",
            "valid_time",
            "status",
            "author",
            "method",
            "evidence",
            "recorded_at",
        ],
        &[
            ("id", "string"),
            ("entity", "string"),
            ("property", "string"),
            ("schema_revision", "string"),
            ("definition_manifest", "string"),
            ("context", "string"),
            ("author", "string"),
            ("method", "string"),
        ],
    )
}
pub fn record_snapshot_schema() -> SchemaDefinition {
    schema(
        "eidetic.RecordSnapshot/v1",
        &[
            "scope_generation",
            "complete",
            "exclusions",
            "sources",
            "definitions",
            "records",
            "lifecycle",
        ],
        &[
            ("scope_generation", "string"),
            ("complete", "bool"),
            ("exclusions", "array"),
            ("sources", "array"),
            ("definitions", "array"),
            ("records", "array"),
            ("lifecycle", "array"),
        ],
    )
}
pub fn record_query_schema() -> SchemaDefinition {
    schema(
        "eidetic.RecordQuery/v1",
        &["operation", "filters"],
        &[("operation", "string"), ("filters", "object")],
    )
}
pub fn record_result_schema() -> SchemaDefinition {
    schema(
        "eidetic.RecordResult/v1",
        &["operation_version", "snapshot", "query", "settings", "rows"],
        &[
            ("operation_version", "string"),
            ("snapshot", "object"),
            ("query", "object"),
            ("settings", "object"),
            ("rows", "array"),
        ],
    )
}
pub(super) fn schema_ref(definition: SchemaDefinition) -> SchemaRef {
    SchemaRef::from_id(ManifestId::of_blob(
        &serde_json::to_vec(&definition).expect("schema serializes"),
    ))
}
/// Seed exact versioned schema payloads as ordinary schema codicils.
pub async fn bootstrap_record_schemas(store: &mut dyn Store) -> crate::Result<()> {
    bootstrap_meta_schema(store).await?;
    for definition in [
        property_definition_schema(),
        property_record_schema(),
        record_snapshot_schema(),
        record_query_schema(),
        record_result_schema(),
    ] {
        let provenance = ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: vec![],
            tooling: Some(format!("eidetic/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp::ZERO,
        };
        let trust = TrustEnvelope {
            level: TrustLevel::CheckpointAccepted,
            signatures: vec![],
            moderation_state: ModerationState::Accepted,
        };
        let id = save_typed(
            store,
            &definition,
            Vec::<BlobSource>::new(),
            PrivacyClass::PublicPortable,
            provenance,
            trust,
            Timestamp::ZERO,
        )
        .await?;
        if id != schema_ref(definition).0 {
            return Err(Error::new("record schema identity mismatch"));
        }
    }
    Ok(())
}
/// Require the declared schema to resolve before validating, unlike tolerant reads.
/// This checks the outer schema; hosts must also invoke intrinsic validators.
pub async fn validate_required_schema<T: TypedPayload>(
    store: &mut dyn Store,
    fetcher: &mut dyn BlobFetcher,
    value: &T,
) -> crate::Result<()> {
    let definition = load_schema(store, fetcher, T::schema_ref().0)
        .await?
        .ok_or_else(|| Error::new("required record schema unavailable"))?;
    validate_payload(&definition, &value.serialize_to_bytes()?)
}
