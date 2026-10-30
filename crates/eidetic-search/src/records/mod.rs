// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. SPDX-License-Identifier: MPL-2.0
//! Deterministic evaluation of already-authorized, complete snapshots.
use eidetic::models::recall::RecallSourceId;
use eidetic::records::*;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
mod operations;

struct Budget {
    left: usize,
}
impl Write for Budget {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.left = self
            .left
            .checked_sub(buf.len())
            .ok_or_else(|| io::Error::other("input limit"))?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn invalid(s: &str) -> ResearchError {
    ResearchError::Invalid(s.into())
}
fn push(
    rows: &mut Vec<RecordRow>,
    row: RecordRow,
    settings: &RecordSettings,
    output: &mut Budget,
) -> Result<(), ResearchError> {
    if rows.len() >= settings.max_rows {
        return Err(ResearchError::LimitExceeded);
    }
    output
        .write_all(b",")
        .map_err(|_| ResearchError::LimitExceeded)?;
    serde_json::to_writer(output, &row).map_err(|_| ResearchError::LimitExceeded)?;
    rows.push(row);
    Ok(())
}
fn row(key: RowValue, records: &[&PropertyRecordV1], sources: Vec<RecallSourceId>) -> RecordRow {
    RecordRow {
        key,
        group: None,
        count: None,
        record_ids: records.iter().map(|r| r.id.clone()).collect(),
        sources,
    }
}
fn matches_source(s: &SourceRecord, f: &RecordFilters) -> bool {
    f.source.as_ref().is_none_or(|v| v == &s.source)
        && f.tag.as_ref().is_none_or(|v| s.tags.contains(v))
}
fn matches_record(r: &PropertyRecordV1, f: &RecordFilters, d: &PropertyDefinitionV1) -> bool {
    f.source.as_ref().is_none_or(|v| v == r.evidence.source())
        && f.entity.as_ref().is_none_or(|v| v == &r.entity)
        && f.property.as_ref().is_none_or(|v| v == &r.property)
        && f.schema_revision
            .as_ref()
            .is_none_or(|v| v == &r.schema_revision)
        && f.status
            .as_ref()
            .is_none_or(|v| r.status.as_ref() == Some(v))
        && f.unit.as_ref().is_none_or(|v| d.unit.as_ref() == Some(v))
        && f.valid_time.as_ref().is_none_or(|v| {
            r.valid_time
                .as_ref()
                .is_some_and(|t| t.start < v.end && v.start < t.end)
        })
}
/// Evaluate frozen inputs. The host must check current admission before and after
/// building the snapshot and again before presenting or reopening evidence.
pub fn evaluate(
    snapshot: &RecordSnapshot,
    query: &RecordQuery,
    settings: &RecordSettings,
) -> Result<RecordResult, ResearchError> {
    if snapshot.sources.len() > settings.max_sources
        || snapshot.records.len() > settings.max_records
        || snapshot.definitions.len() > settings.max_records
        || snapshot.lifecycle.len() > settings.max_records
    {
        return Err(ResearchError::LimitExceeded);
    }
    let mut budget = Budget {
        left: settings.max_input_bytes,
    };
    serde_json::to_writer(&mut budget, &(snapshot, query, settings))
        .map_err(|_| ResearchError::LimitExceeded)?;
    if !snapshot.complete || !snapshot.exclusions.is_empty() {
        return Err(ResearchError::Incomplete);
    }
    if snapshot.scope_generation.trim().is_empty() {
        return Err(invalid("missing scope generation"));
    }
    if let Some(interval) = &query.filters.valid_time {
        interval.validate()?;
    }
    let f = &query.filters;
    if matches!(
        query.operation,
        RecordOperation::SourceLookup
            | RecordOperation::CountByProducer
            | RecordOperation::SourceTimeline
            | RecordOperation::TagMembership
    ) && (f.entity.is_some()
        || f.property.is_some()
        || f.schema_revision.is_some()
        || f.status.is_some()
        || f.valid_time.is_some()
        || f.unit.is_some())
    {
        return Err(ResearchError::Unsupported(
            "record filters do not apply to source operations".into(),
        ));
    }
    let mut sources = BTreeMap::new();
    for s in &snapshot.sources {
        if s.source.source.trim().is_empty()
            || s.version.trim().is_empty()
            || s.producer.trim().is_empty()
            || s.tags
                .iter()
                .any(|t| t.owner.trim().is_empty() || t.concept.trim().is_empty())
            || sources.insert(&s.source, s).is_some()
        {
            return Err(invalid("invalid or repeated source"));
        }
    }
    let mut definitions = BTreeMap::new();
    for d in &snapshot.definitions {
        d.validate()?;
        if definitions.insert((&d.id, &d.revision), d).is_some() {
            return Err(invalid("repeated definition"));
        }
    }
    let mut observations = BTreeMap::new();
    for o in &snapshot.lifecycle {
        if o.lifecycle_ref.trim().is_empty()
            || observations.insert(&o.record_id, &o.state).is_some()
        {
            return Err(invalid("repeated lifecycle observation"));
        }
    }
    let mut records = BTreeMap::new();
    let mut validation = Budget {
        left: settings.max_validation_bytes,
    };
    for r in &snapshot.records {
        let d = definitions
            .get(&(&r.property, &r.schema_revision))
            .ok_or_else(|| invalid("missing definition"))?;
        serde_json::to_writer(&mut validation, d).map_err(|_| ResearchError::LimitExceeded)?;
        r.validate(d)?;
        let source = sources
            .get(r.evidence.source())
            .ok_or(ResearchError::Denied)?;
        if source.version != r.evidence.version() {
            return Err(ResearchError::Unavailable);
        }
        match observations.get(&r.id) {
            Some(LifecycleState::Active) => {}
            Some(LifecycleState::Withdrawn) => return Err(ResearchError::Withdrawn),
            Some(LifecycleState::Denied) => return Err(ResearchError::Denied),
            Some(LifecycleState::Unavailable) => return Err(ResearchError::Unavailable),
            None => return Err(invalid("missing lifecycle observation")),
        }
        if let Some(previous) = records.insert(&r.id, r) {
            if previous != r {
                return Err(invalid("conflicting assertion projection"));
            }
        }
    }
    if observations.keys().any(|id| !records.contains_key(id)) {
        return Err(invalid("orphan lifecycle observation"));
    }
    let selected_sources: Vec<_> = sources
        .values()
        .copied()
        .filter(|s| matches_source(s, &query.filters))
        .collect();
    let admitted: BTreeSet<_> = selected_sources.iter().map(|s| &s.source).collect();
    let selected: Vec<_> = records
        .values()
        .copied()
        .filter(|r| {
            admitted.contains(r.evidence.source())
                && matches_record(
                    r,
                    &query.filters,
                    definitions[&(&r.property, &r.schema_revision)],
                )
        })
        .collect();
    let rows = operations::run(query, settings, &selected_sources, &selected, &definitions)?;
    Ok(RecordResult {
        operation_version: "eidetic.structured-records/v1".into(),
        snapshot: snapshot.clone(),
        query: query.clone(),
        settings: settings.clone(),
        rows,
    })
}
