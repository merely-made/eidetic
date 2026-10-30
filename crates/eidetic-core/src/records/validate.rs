use super::*;
use std::collections::BTreeSet;
fn nonempty(s: &str) -> bool {
    !s.trim().is_empty()
}
fn invalid(s: &str) -> ResearchError {
    ResearchError::Invalid(s.into())
}
impl CivilDate {
    pub fn validate(&self) -> Result<(), ResearchError> {
        let leap = self.year % 4 == 0 && (self.year % 100 != 0 || self.year % 400 == 0);
        let days = match self.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => 0,
        };
        if !(1..=9999).contains(&self.year) || self.day == 0 || self.day > days {
            return Err(invalid("invalid civil date"));
        }
        Ok(())
    }
    fn days_since_epoch(&self) -> i64 {
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let d = i64::from(self.day);
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
        era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
    }
}
impl OffsetTimestamp {
    pub fn utc_seconds(&self) -> Result<i64, ResearchError> {
        self.date.validate()?;
        if self.hour > 23
            || self.minute > 59
            || self.second > 59
            || !(-1439..=1439).contains(&self.offset_minutes)
        {
            return Err(invalid("invalid offset timestamp"));
        }
        Ok(self.date.days_since_epoch() * 86400
            + i64::from(self.hour) * 3600
            + i64::from(self.minute) * 60
            + i64::from(self.second)
            - i64::from(self.offset_minutes) * 60)
    }
}
impl TimeInterval {
    pub fn validate(&self) -> Result<(), ResearchError> {
        if self.start >= self.end {
            Err(invalid("invalid interval"))
        } else {
            Ok(())
        }
    }
}
impl PropertyDefinitionV1 {
    /// Content-addressed ID of the exact serialized definition payload.
    pub fn manifest_id(&self) -> crate::Result<ManifestId> {
        Ok(ManifestId::of_blob(&self.serialize_to_bytes()?))
    }

    pub fn validate(&self) -> Result<(), ResearchError> {
        if !nonempty(&self.id)
            || !(self.id.contains('.') || self.id.contains(':'))
            || !nonempty(&self.revision)
            || self.unit.as_ref().is_some_and(|u| !nonempty(u))
        {
            return Err(invalid("invalid property definition"));
        }
        if self.value_type == ValueType::Decimal && self.unit.is_none() {
            return Err(invalid("decimal requires unit"));
        }
        if !matches!(self.value_type, ValueType::Decimal | ValueType::Integer)
            && self.unit.is_some()
        {
            return Err(invalid("unit on nonnumeric property"));
        }
        let mut seen = BTreeSet::new();
        if self
            .statuses
            .iter()
            .any(|s| !nonempty(s) || !seen.insert(s))
        {
            return Err(invalid("invalid status definitions"));
        }
        Ok(())
    }
}
impl PropertyRecordV1 {
    pub fn validate(&self, definition: &PropertyDefinitionV1) -> Result<(), ResearchError> {
        definition.validate()?;
        if definition
            .manifest_id()
            .map_err(|_| invalid("cannot serialize definition"))?
            != self.definition_manifest
        {
            return Err(invalid("definition manifest does not match assertion"));
        }
        if [
            &self.id,
            &self.entity,
            &self.property,
            &self.schema_revision,
            &self.context,
            &self.author,
            &self.method,
        ]
        .iter()
        .any(|s| !nonempty(s))
            || self.property != definition.id
            || self.schema_revision != definition.revision
        {
            return Err(invalid("invalid assertion identity or schema"));
        }
        if self
            .status
            .as_ref()
            .is_some_and(|s| !definition.statuses.contains(s))
        {
            return Err(invalid("unknown status"));
        }
        if let Some(t) = &self.valid_time {
            t.validate()?;
        }
        let valid = match (&self.value, &definition.value_type) {
            (PropertyValue::Text(s), ValueType::Text) => nonempty(s),
            (PropertyValue::Boolean(_), ValueType::Boolean)
            | (PropertyValue::Integer(_), ValueType::Integer) => true,
            (PropertyValue::Decimal { scale, unit, .. }, ValueType::Decimal) => {
                *scale <= 18 && Some(unit) == definition.unit.as_ref()
            }
            (PropertyValue::CivilDate(d), ValueType::CivilDate) => {
                d.validate()?;
                true
            }
            (PropertyValue::Timestamp(t), ValueType::Timestamp) => {
                t.utc_seconds()?;
                true
            }
            _ => false,
        };
        if !valid {
            return Err(invalid("value does not conform to definition"));
        }
        match &self.evidence {
            Evidence::Passage {
                source,
                version,
                selector,
                ..
            } if nonempty(&source.source) && nonempty(version) && selector.start < selector.end => {
            }
            Evidence::Field {
                source,
                version,
                record_id,
                field,
            } if nonempty(&source.source)
                && nonempty(version)
                && nonempty(record_id)
                && nonempty(field) => {}
            _ => return Err(invalid("invalid evidence reference")),
        }
        Ok(())
    }
}
