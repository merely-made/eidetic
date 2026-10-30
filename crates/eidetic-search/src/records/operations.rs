use super::output::{Key, Numeric, Value, emit};
use super::*;

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
    let equal = match (&a.value, &b.value) {
        (PropertyValue::Timestamp(x), PropertyValue::Timestamp(y)) => {
            x.utc_seconds()? == y.utc_seconds()?
        }
        (x, y) => x == y,
    };
    if equal {
        Ok(Comparison::Equal)
    } else if d.cardinality == Cardinality::Multiple {
        Ok(Comparison::Distinct)
    } else {
        Ok(Comparison::Disagreement)
    }
}
fn source_bytes(s: &RecallSourceId) -> Option<usize> {
    s.source
        .len()
        .checked_add(s.owner.as_ref().map_or(0, String::len))?
        .checked_add(s.node.as_ref().map_or(0, String::len))
}
fn value_bytes(value: &PropertyValue) -> usize {
    match value {
        PropertyValue::Text(v) => v.len(),
        PropertyValue::Decimal { unit, .. } => unit.len(),
        _ => 0,
    }
}
fn charge_pair(
    a: &PropertyRecordV1,
    b: &PropertyRecordV1,
    op: &RecordOperation,
    remaining: &mut usize,
) -> Result<(), ResearchError> {
    let cost = match op {
        RecordOperation::JoinEntity => a.entity.len().checked_add(b.entity.len()),
        RecordOperation::JoinSource => source_bytes(a.evidence.source())
            .and_then(|x| source_bytes(b.evidence.source()).and_then(|y| x.checked_add(y))),
        RecordOperation::Compare => [a, b].iter().try_fold(0usize, |n, r| {
            [
                r.entity.len(),
                r.property.len(),
                r.schema_revision.len(),
                r.context.len(),
                r.status.as_ref().map_or(0, String::len),
                value_bytes(&r.value),
            ]
            .into_iter()
            .try_fold(n, usize::checked_add)
        }),
        _ => Some(0),
    }
    .ok_or(ResearchError::LimitExceeded)?;
    *remaining = remaining
        .checked_sub(cost)
        .ok_or(ResearchError::LimitExceeded)?;
    Ok(())
}
fn pair_work(n: usize, settings: &RecordSettings) -> Result<usize, ResearchError> {
    let pairs = n
        .checked_mul(n.saturating_sub(1))
        .and_then(|x| x.checked_div(2))
        .ok_or(ResearchError::LimitExceeded)?;
    settings
        .max_work
        .checked_sub(pairs)
        .ok_or(ResearchError::LimitExceeded)
}
fn source_refs<'a>(records: &[&'a PropertyRecordV1]) -> Vec<&'a RecallSourceId> {
    records
        .iter()
        .map(|r| r.evidence.source())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
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
                    Key::Time(s.captured_at)
                } else {
                    Key::Source(&s.source)
                };
                emit(
                    &mut rows,
                    key,
                    None,
                    None,
                    &[],
                    vec![&s.source],
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
            let mut groups: BTreeMap<&str, Vec<&RecallSourceId>> = BTreeMap::new();
            for s in sources {
                if !groups.contains_key(s.producer.as_str()) && groups.len() >= settings.max_rows {
                    return Err(ResearchError::LimitExceeded);
                }
                groups.entry(&s.producer).or_default().push(&s.source);
            }
            for (producer, refs) in groups {
                emit(
                    &mut rows,
                    Key::Text(producer),
                    None,
                    Some(refs.len() as u64),
                    &[],
                    refs,
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
                emit(
                    &mut rows,
                    Key::Source(source),
                    None,
                    Some(rs.len() as u64),
                    &rs,
                    vec![source],
                    settings,
                    &mut output,
                )?;
            }
        }
        RecordOperation::PropertyLookup | RecordOperation::RecordTimeline => {
            for r in records {
                let key = if matches!(query.operation, RecordOperation::RecordTimeline) {
                    Key::Time(r.recorded_at)
                } else {
                    Key::Value(Value::Existing(&r.value))
                };
                emit(
                    &mut rows,
                    key,
                    None,
                    None,
                    &[*r],
                    vec![r.evidence.source()],
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
                let first = rs[0];
                emit(
                    &mut rows,
                    Key::Value(Value::Existing(&first.value)),
                    Some(first),
                    None,
                    rs,
                    source_refs(rs),
                    settings,
                    &mut output,
                )?;
            }
        }
        RecordOperation::JoinEntity | RecordOperation::JoinSource | RecordOperation::Compare => {
            let mut remaining = pair_work(records.len(), settings)?;
            for (i, a) in records.iter().enumerate() {
                for b in &records[i + 1..] {
                    charge_pair(a, b, &query.operation, &mut remaining)?;
                    let key = match query.operation {
                        RecordOperation::JoinEntity if a.entity == b.entity => {
                            Some(Key::Pair(&a.id, &b.id))
                        }
                        RecordOperation::JoinSource
                            if a.evidence.source() == b.evidence.source() =>
                        {
                            Some(Key::Pair(&a.id, &b.id))
                        }
                        RecordOperation::Compare => Some(Key::Comparison(relation(
                            a,
                            b,
                            definitions[&(&a.property, &a.schema_revision)],
                        )?)),
                        _ => None,
                    };
                    if let Some(key) = key {
                        let pair = [*a, *b];
                        emit(
                            &mut rows,
                            key,
                            None,
                            None,
                            &pair,
                            source_refs(&pair),
                            settings,
                            &mut output,
                        )?;
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
                        return Err(invalid("sum requires one property schema"));
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
                            return Err(invalid("sum requires compatible numeric units and scale"));
                        }
                    };
                    sum = sum.checked_add(value).ok_or(ResearchError::Overflow)?;
                }
                let value = match &first.value {
                    PropertyValue::Integer(_) => Numeric::Integer(sum),
                    PropertyValue::Decimal { scale, unit, .. } => Numeric::Decimal {
                        coefficient: sum,
                        scale: *scale,
                        unit,
                    },
                    _ => unreachable!(),
                };
                emit(
                    &mut rows,
                    Key::Value(Value::Numeric(value)),
                    None,
                    None,
                    records,
                    source_refs(records),
                    settings,
                    &mut output,
                )?;
            }
        }
    }
    Ok(rows)
}
