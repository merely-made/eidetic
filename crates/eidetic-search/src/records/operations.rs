use super::*;
use serde::Serialize;

#[derive(Serialize)]
enum PairKey<'a> {
    Pair(&'a str, &'a str),
    Comparison(Comparison),
}
#[derive(Serialize)]
struct PairView<'a> {
    key: &'a PairKey<'a>,
    group: Option<()>,
    count: Option<u64>,
    record_ids: [&'a str; 2],
    sources: Vec<&'a RecallSourceId>,
}
fn preflight_pair(
    key: &PairKey<'_>,
    a: &PropertyRecordV1,
    b: &PropertyRecordV1,
    output: &Budget,
) -> Result<(), ResearchError> {
    let mut sources = vec![a.evidence.source(), b.evidence.source()];
    sources.sort();
    sources.dedup();
    let view = PairView {
        key,
        group: None,
        count: None,
        record_ids: [&a.id, &b.id],
        sources,
    };
    let mut remaining = Budget { left: output.left };
    remaining
        .write_all(b",")
        .map_err(|_| ResearchError::LimitExceeded)?;
    serde_json::to_writer(&mut remaining, &view).map_err(|_| ResearchError::LimitExceeded)
}

type Definitions<'a> = BTreeMap<(&'a String, &'a String), &'a PropertyDefinitionV1>;
fn value_key(value: &PropertyValue) -> Result<String, ResearchError> {
    match value {
        PropertyValue::Timestamp(t) => Ok(format!("timestamp:{:020}", t.utc_seconds()?)),
        _ => serde_json::to_string(value).map_err(|e| invalid(&e.to_string())),
    }
}
fn relation(
    a: &PropertyRecordV1,
    b: &PropertyRecordV1,
    d: &PropertyDefinitionV1,
) -> Result<Comparison, ResearchError> {
    if a.entity != b.entity
        || a.property != b.property
        || a.schema_revision != b.schema_revision
        || a.context != b.context
    {
        return Ok(Comparison::Distinct);
    }
    if a.status.is_none()
        || a.status != b.status
        || a.valid_time.is_none()
        || b.valid_time.is_none()
    {
        return Ok(Comparison::NonComparable);
    }
    let (x, y) = (
        a.valid_time.as_ref().unwrap(),
        b.valid_time.as_ref().unwrap(),
    );
    if x.start >= y.end || y.start >= x.end {
        return Ok(Comparison::Distinct);
    }
    if let (PropertyValue::Decimal { scale: x, .. }, PropertyValue::Decimal { scale: y, .. }) =
        (&a.value, &b.value)
    {
        if x != y {
            return Ok(Comparison::NonComparable);
        }
    }
    if value_key(&a.value)? == value_key(&b.value)? {
        Ok(Comparison::Equal)
    } else if d.cardinality == Cardinality::Multiple {
        Ok(Comparison::Distinct)
    } else {
        Ok(Comparison::Disagreement)
    }
}
fn work(n: usize, settings: &RecordSettings) -> Result<(), ResearchError> {
    if n.checked_mul(n.saturating_sub(1))
        .and_then(|x| x.checked_div(2))
        .is_none_or(|x| x > settings.max_work)
    {
        Err(ResearchError::LimitExceeded)
    } else {
        Ok(())
    }
}
pub(super) fn run(
    query: &RecordQuery,
    settings: &RecordSettings,
    sources: &[&SourceRecord],
    records: &[&PropertyRecordV1],
    definitions: &Definitions<'_>,
) -> Result<Vec<RecordRow>, ResearchError> {
    let mut rows = Vec::new();
    let mut output = Budget {
        left: settings.max_output_bytes,
    };
    match query.operation {
        RecordOperation::SourceLookup
        | RecordOperation::SourceTimeline
        | RecordOperation::TagMembership => {
            if matches!(query.operation, RecordOperation::TagMembership)
                && query.filters.tag.is_none()
            {
                return Err(invalid("tag identity required"));
            }
            for s in sources {
                let key = if matches!(query.operation, RecordOperation::SourceTimeline) {
                    RowValue::Time(s.captured_at)
                } else {
                    RowValue::Source(s.source.clone())
                };
                push(
                    &mut rows,
                    row(key, &[], vec![s.source.clone()]),
                    settings,
                    &mut output,
                )?;
            }
            if matches!(query.operation, RecordOperation::SourceTimeline) {
                rows.sort_by(|a, b| {
                    if let (RowValue::Time(x), RowValue::Time(y)) = (&a.key, &b.key) {
                        x.cmp(y).then(a.sources.cmp(&b.sources))
                    } else {
                        std::cmp::Ordering::Equal
                    }
                });
            }
        }
        RecordOperation::CountByProducer => {
            let mut groups: BTreeMap<&str, Vec<RecallSourceId>> = BTreeMap::new();
            for s in sources {
                if !groups.contains_key(s.producer.as_str()) && groups.len() >= settings.max_rows {
                    return Err(ResearchError::LimitExceeded);
                }
                groups
                    .entry(&s.producer)
                    .or_default()
                    .push(s.source.clone());
            }
            for (producer, refs) in groups {
                push(
                    &mut rows,
                    RecordRow {
                        key: RowValue::Text(producer.into()),
                        group: None,
                        count: Some(refs.len() as u64),
                        record_ids: vec![],
                        sources: refs,
                    },
                    settings,
                    &mut output,
                )?;
            }
        }
        RecordOperation::CountBySource => {
            let mut groups: BTreeMap<&RecallSourceId, Vec<&PropertyRecordV1>> = BTreeMap::new();
            for r in records {
                if !groups.contains_key(r.evidence.source()) && groups.len() >= settings.max_rows {
                    return Err(ResearchError::LimitExceeded);
                }
                groups.entry(r.evidence.source()).or_default().push(r);
            }
            for (source, rs) in groups {
                let mut result = row(RowValue::Source(source.clone()), &rs, vec![source.clone()]);
                result.count = Some(rs.len() as u64);
                push(&mut rows, result, settings, &mut output)?;
            }
        }
        RecordOperation::PropertyLookup | RecordOperation::RecordTimeline => {
            for r in records {
                let key = if matches!(query.operation, RecordOperation::RecordTimeline) {
                    RowValue::Time(r.recorded_at)
                } else {
                    RowValue::Value(r.value.clone())
                };
                push(
                    &mut rows,
                    row(key, &[*r], vec![r.evidence.source().clone()]),
                    settings,
                    &mut output,
                )?;
            }
            if matches!(query.operation, RecordOperation::RecordTimeline) {
                rows.sort_by(|a, b| {
                    if let (RowValue::Time(x), RowValue::Time(y)) = (&a.key, &b.key) {
                        x.cmp(y).then(a.record_ids.cmp(&b.record_ids))
                    } else {
                        std::cmp::Ordering::Equal
                    }
                });
            }
        }
        RecordOperation::GroupValues => {
            let mut groups = BTreeMap::new();
            for r in records {
                let k = (
                    &r.entity,
                    &r.property,
                    &r.schema_revision,
                    &r.context,
                    &r.status,
                    &r.valid_time,
                    value_key(&r.value)?,
                );
                if !groups.contains_key(&k) && groups.len() >= settings.max_rows {
                    return Err(ResearchError::LimitExceeded);
                }
                groups.entry(k).or_insert_with(Vec::new).push(*r);
            }
            for rs in groups.values() {
                let refs = rs
                    .iter()
                    .map(|r| r.evidence.source().clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                let first = rs[0];
                let mut result = row(RowValue::Value(first.value.clone()), rs, refs);
                result.group = Some(PropertyGroupKey {
                    entity: first.entity.clone(),
                    property: first.property.clone(),
                    schema_revision: first.schema_revision.clone(),
                    context: first.context.clone(),
                    status: first.status.clone(),
                    valid_time: first.valid_time.clone(),
                    value: first.value.clone(),
                });
                push(&mut rows, result, settings, &mut output)?;
            }
        }
        RecordOperation::JoinEntity | RecordOperation::JoinSource | RecordOperation::Compare => {
            work(records.len(), settings)?;
            for (i, a) in records.iter().enumerate() {
                for b in &records[i + 1..] {
                    let key = match query.operation {
                        RecordOperation::JoinEntity if a.entity == b.entity => {
                            Some(PairKey::Pair(&a.id, &b.id))
                        }
                        RecordOperation::JoinSource
                            if a.evidence.source() == b.evidence.source() =>
                        {
                            Some(PairKey::Pair(&a.id, &b.id))
                        }
                        RecordOperation::Compare => Some(PairKey::Comparison(relation(
                            a,
                            b,
                            definitions[&(&a.property, &a.schema_revision)],
                        )?)),
                        _ => None,
                    };
                    if let Some(key) = key {
                        if rows.len() >= settings.max_rows {
                            return Err(ResearchError::LimitExceeded);
                        }
                        preflight_pair(&key, a, b, &output)?;
                        let key = match key {
                            PairKey::Pair(x, y) => RowValue::Pair(x.into(), y.into()),
                            PairKey::Comparison(c) => RowValue::Comparison(c),
                        };
                        let refs = [a.evidence.source().clone(), b.evidence.source().clone()]
                            .into_iter()
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect();
                        push(&mut rows, row(key, &[*a, *b], refs), settings, &mut output)?;
                    }
                }
            }
        }
        RecordOperation::Sum => {
            if let Some(first) = records.first() {
                let definition = definitions[&(&first.property, &first.schema_revision)];
                let mut sum = 0i64;
                for r in records {
                    if r.property != first.property || r.schema_revision != first.schema_revision {
                        return Err(ResearchError::Invalid(
                            "sum requires one property schema".into(),
                        ));
                    }
                    let value = match (&first.value, &r.value) {
                        (PropertyValue::Integer(_), PropertyValue::Integer(v)) => *v,
                        (
                            PropertyValue::Decimal {
                                scale: a, unit: u, ..
                            },
                            PropertyValue::Decimal {
                                coefficient,
                                scale: b,
                                unit: v,
                            },
                        ) if a == b && u == v && definition.unit.as_ref() == Some(u) => {
                            *coefficient
                        }
                        _ => {
                            return Err(ResearchError::Invalid(
                                "sum requires compatible numeric units and scale".into(),
                            ));
                        }
                    };
                    sum = sum.checked_add(value).ok_or(ResearchError::Overflow)?;
                }
                let value = match &first.value {
                    PropertyValue::Integer(_) => PropertyValue::Integer(sum),
                    PropertyValue::Decimal { scale, unit, .. } => PropertyValue::Decimal {
                        coefficient: sum,
                        scale: *scale,
                        unit: unit.clone(),
                    },
                    _ => unreachable!(),
                };
                let refs = records
                    .iter()
                    .map(|r| r.evidence.source().clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                push(
                    &mut rows,
                    row(RowValue::Value(value), records, refs),
                    settings,
                    &mut output,
                )?;
            }
        }
    }
    Ok(rows)
}
