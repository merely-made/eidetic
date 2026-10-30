// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. SPDX-License-Identifier: MPL-2.0
//! Portable, explicit assertions and frozen structured-query inputs.
//! Hosts own admission and journal observations; these records grant no access.
use crate::models::recall::{PassageSelector, RecallSourceId};
use crate::{Hash, ManifestId, SchemaRef, TypedPayload};
use serde::{Deserialize, Serialize};

mod schemas;
mod validate;
pub use schemas::{
    bootstrap_record_schemas, property_definition_schema, property_record_schema,
    record_query_schema, record_result_schema, record_snapshot_schema, validate_required_schema,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ValueType {
    Text,
    Boolean,
    Integer,
    Decimal,
    CivilDate,
    Timestamp,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cardinality {
    Single,
    Multiple,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyDefinitionV1 {
    pub id: String,
    pub revision: String,
    pub value_type: ValueType,
    pub unit: Option<String>,
    pub cardinality: Cardinality,
    pub statuses: Vec<String>,
}
/// Decimal is an integer coefficient and decimal scale. No implicit conversion.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PropertyValue {
    Text(String),
    Boolean(bool),
    Integer(i64),
    Decimal {
        coefficient: i64,
        scale: u8,
        unit: String,
    },
    CivilDate(CivilDate),
    Timestamp(OffsetTimestamp),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CivilDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}
/// An offset-qualified local time; no machine timezone is consulted.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OffsetTimestamp {
    pub date: CivilDate,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub offset_minutes: i16,
}
/// Half-open interval in UTC seconds; absent interval means unknown valid time.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TimeInterval {
    pub start: i64,
    pub end: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Evidence {
    Passage {
        source: RecallSourceId,
        version: String,
        hash: Hash,
        selector: PassageSelector,
    },
    Field {
        source: RecallSourceId,
        version: String,
        record_id: String,
        field: String,
    },
}
impl Evidence {
    pub fn source(&self) -> &RecallSourceId {
        match self {
            Self::Passage { source, .. } | Self::Field { source, .. } => source,
        }
    }
    pub fn version(&self) -> &str {
        match self {
            Self::Passage { version, .. } | Self::Field { version, .. } => version,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyRecordV1 {
    pub id: String,
    pub entity: String,
    pub property: String,
    pub schema_revision: String,
    pub definition_manifest: ManifestId,
    pub context: String,
    pub value: PropertyValue,
    pub valid_time: Option<TimeInterval>,
    pub status: Option<String>,
    pub author: String,
    pub method: String,
    pub evidence: Evidence,
    pub recorded_at: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TagIdentity {
    pub owner: String,
    pub concept: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRecord {
    pub source: RecallSourceId,
    pub version: String,
    pub producer: String,
    pub captured_at: Option<i64>,
    pub tags: Vec<TagIdentity>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleState {
    Active,
    Withdrawn,
    Denied,
    Unavailable,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleObservation {
    pub record_id: String,
    pub lifecycle_ref: String,
    pub state: LifecycleState,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordSnapshot {
    pub scope_generation: String,
    pub complete: bool,
    pub exclusions: Vec<String>,
    pub sources: Vec<SourceRecord>,
    pub definitions: Vec<PropertyDefinitionV1>,
    pub records: Vec<PropertyRecordV1>,
    pub lifecycle: Vec<LifecycleObservation>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordFilters {
    pub source: Option<RecallSourceId>,
    pub entity: Option<String>,
    pub property: Option<String>,
    pub schema_revision: Option<String>,
    pub status: Option<String>,
    pub valid_time: Option<TimeInterval>,
    pub unit: Option<String>,
    pub tag: Option<TagIdentity>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordOperation {
    SourceLookup,
    CountByProducer,
    CountBySource,
    SourceTimeline,
    RecordTimeline,
    TagMembership,
    PropertyLookup,
    GroupValues,
    JoinEntity,
    JoinSource,
    Compare,
    Sum,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordQuery {
    pub operation: RecordOperation,
    pub filters: RecordFilters,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordSettings {
    pub max_sources: usize,
    pub max_records: usize,
    /// Pair operations plus bytes inspected for pair identity and value comparisons.
    pub max_work: usize,
    /// Cumulative schema bytes checked while validating assertions.
    pub max_validation_bytes: usize,
    pub max_rows: usize,
    pub max_input_bytes: usize,
    /// Maximum serialized row bytes, including per-row separators.
    pub max_output_bytes: usize,
}
impl Default for RecordSettings {
    fn default() -> Self {
        Self {
            max_sources: 10_000,
            max_records: 100_000,
            max_work: 1_000_000,
            max_validation_bytes: 16 * 1024 * 1024,
            max_rows: 100_000,
            max_input_bytes: 16 * 1024 * 1024,
            max_output_bytes: 16 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RowValue {
    Source(RecallSourceId),
    Text(String),
    Time(Option<i64>),
    Value(PropertyValue),
    Count(u64),
    Comparison(Comparison),
    Pair(String, String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Comparison {
    Equal,
    Disagreement,
    Distinct,
    NonComparable,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyGroupKey {
    pub entity: String,
    pub property: String,
    pub schema_revision: String,
    pub context: String,
    pub status: Option<String>,
    pub valid_time: Option<TimeInterval>,
    pub value: PropertyValue,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordRow {
    pub key: RowValue,
    pub group: Option<PropertyGroupKey>,
    pub count: Option<u64>,
    pub record_ids: Vec<String>,
    pub sources: Vec<RecallSourceId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordResult {
    pub operation_version: String,
    pub snapshot: RecordSnapshot,
    pub query: RecordQuery,
    pub settings: RecordSettings,
    pub rows: Vec<RecordRow>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResearchError {
    Denied,
    Withdrawn,
    Unavailable,
    Invalid(String),
    Unsupported(String),
    ChangedDuringOperation,
    LimitExceeded,
    Incomplete,
    Overflow,
}

macro_rules! payload {
    ($ty:ty, $schema:ident) => {
        impl TypedPayload for $ty {
            fn schema_ref() -> SchemaRef {
                schemas::schema_ref($schema())
            }
        }
    };
}
payload!(PropertyDefinitionV1, property_definition_schema);
payload!(PropertyRecordV1, property_record_schema);
payload!(RecordSnapshot, record_snapshot_schema);
payload!(RecordQuery, record_query_schema);
payload!(RecordResult, record_result_schema);
