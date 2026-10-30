//! Borrowed row preflight, before materializing any output-owned strings.
use super::*;
use serde::{Serialize, Serializer, ser::SerializeSeq};

#[derive(Serialize)]
pub(super) enum Numeric<'a> {
    Integer(i64),
    Decimal {
        coefficient: i64,
        scale: u8,
        unit: &'a str,
    },
}
pub(super) enum Value<'a> {
    Existing(&'a PropertyValue),
    Numeric(Numeric<'a>),
}
impl Serialize for Value<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Existing(v) => v.serialize(serializer),
            Self::Numeric(v) => v.serialize(serializer),
        }
    }
}
impl Value<'_> {
    fn into_owned(self) -> PropertyValue {
        match self {
            Self::Existing(value) => value.clone(),
            Self::Numeric(Numeric::Integer(v)) => PropertyValue::Integer(v),
            Self::Numeric(Numeric::Decimal {
                coefficient,
                scale,
                unit,
            }) => PropertyValue::Decimal {
                coefficient,
                scale,
                unit: unit.into(),
            },
        }
    }
}
#[derive(Serialize)]
pub(super) enum Key<'a> {
    Source(&'a RecallSourceId),
    Text(&'a str),
    Time(Option<i64>),
    Value(Value<'a>),
    Comparison(Comparison),
    Pair(&'a str, &'a str),
}
impl Key<'_> {
    fn into_owned(self) -> RowValue {
        match self {
            Self::Source(v) => RowValue::Source(v.clone()),
            Self::Text(v) => RowValue::Text(v.into()),
            Self::Time(v) => RowValue::Time(v),
            Self::Value(v) => RowValue::Value(v.into_owned()),
            Self::Comparison(v) => RowValue::Comparison(v),
            Self::Pair(a, b) => RowValue::Pair(a.into(), b.into()),
        }
    }
}
#[derive(Serialize)]
struct Group<'a> {
    entity: &'a str,
    property: &'a str,
    schema_revision: &'a str,
    context: &'a str,
    status: &'a Option<String>,
    valid_time: &'a Option<TimeInterval>,
    value: &'a PropertyValue,
}
impl<'a> From<&'a PropertyRecordV1> for Group<'a> {
    fn from(r: &'a PropertyRecordV1) -> Self {
        Self {
            entity: &r.entity,
            property: &r.property,
            schema_revision: &r.schema_revision,
            context: &r.context,
            status: &r.status,
            valid_time: &r.valid_time,
            value: &r.value,
        }
    }
}
struct Ids<'a, 'b>(&'a [&'b PropertyRecordV1]);
impl Serialize for Ids<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for record in self.0 {
            seq.serialize_element(&record.id)?;
        }
        seq.end()
    }
}
#[derive(Serialize)]
struct RowView<'a, 'b> {
    key: &'a Key<'b>,
    group: Option<Group<'b>>,
    count: Option<u64>,
    record_ids: Ids<'a, 'b>,
    sources: &'a [&'b RecallSourceId],
}
/// The serialized view has precisely RecordRow's shape, but borrows every
/// potentially large field. A failed check never clones an output string.
pub(super) fn emit<'a>(
    rows: &mut Vec<RecordRow>,
    key: Key<'a>,
    group: Option<&'a PropertyRecordV1>,
    count: Option<u64>,
    records: &[&'a PropertyRecordV1],
    sources: Vec<&'a RecallSourceId>,
    settings: &RecordSettings,
    output: &mut Budget,
) -> Result<(), ResearchError> {
    if rows.len() >= settings.max_rows {
        return Err(ResearchError::LimitExceeded);
    }
    let view = RowView {
        key: &key,
        group: group.map(Group::from),
        count,
        record_ids: Ids(records),
        sources: &sources,
    };
    let mut remaining = Budget { left: output.left };
    remaining
        .write_all(b",")
        .map_err(|_| ResearchError::LimitExceeded)?;
    serde_json::to_writer(&mut remaining, &view).map_err(|_| ResearchError::LimitExceeded)?;
    let group = group.map(|r| PropertyGroupKey {
        entity: r.entity.clone(),
        property: r.property.clone(),
        schema_revision: r.schema_revision.clone(),
        context: r.context.clone(),
        status: r.status.clone(),
        valid_time: r.valid_time.clone(),
        value: r.value.clone(),
    });
    let row = RecordRow {
        key: key.into_owned(),
        group,
        count,
        record_ids: records.iter().map(|r| r.id.clone()).collect(),
        sources: sources.into_iter().cloned().collect(),
    };
    push(rows, row, settings, output)
}
