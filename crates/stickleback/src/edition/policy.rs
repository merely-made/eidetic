// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Body policy for edition operations.

use p2panda_core::operation::validate_operation;
use p2panda_core::{Hash, Operation, Topic};

use super::{EditionBody, EditionError, EditionExt, EditionLimits};
use crate::processor::{Admission, OperationPolicy, Reject, StoreTarget};

/// Replication topic of one edition log.
pub fn edition_topic(log_key: &[u8; 32]) -> Topic {
    let mut input = b"stickleback/edition/v1".to_vec();
    input.extend_from_slice(log_key);
    Topic::from(Hash::digest(input))
}

/// Structural and body checks shared by admission and chain verification.
///
/// `expected` fixes the log key; `None` accepts any signer, for stores that
/// hold many logs.
///
/// # Errors
/// Refuses a bad signature or body hash, a foreign signer, a missing or
/// malformed body, or a `fork_of` anywhere but a log's first edition.
pub fn check_edition_operation(
    operation: &Operation<EditionExt>,
    expected: Option<&[u8; 32]>,
    limits: EditionLimits,
) -> Result<EditionBody, EditionError> {
    validate_operation(operation).map_err(|e| EditionError::Invalid(e.to_string()))?;
    if operation.hash != operation.header.hash() {
        return Err(EditionError::Invalid(
            "operation id does not match its signed header".into(),
        ));
    }
    if operation.header.extensions.version != 1 {
        return Err(EditionError::Body("unsupported edition version".into()));
    }
    if expected.is_some_and(|key| key != operation.header.verifying_key.as_bytes()) {
        return Err(EditionError::WrongAuthor);
    }
    let bytes = operation
        .body
        .as_ref()
        .ok_or_else(|| EditionError::Body("edition has no body".into()))?
        .to_bytes();
    let body = EditionBody::decode(&bytes, limits)?;
    if operation.header.seq_num > 0 && body.fork_of.is_some() {
        return Err(EditionError::Body(
            "only the first edition of a log names a fork source".into(),
        ));
    }
    Ok(body)
}

/// Admission policy for edition operations: one log per key, one topic per key.
#[derive(Clone, Copy, Debug, Default)]
pub struct EditionPolicy {
    /// Restrict the store to one log key.
    pub only_key: Option<[u8; 32]>,
    /// Body bounds.
    pub limits: EditionLimits,
}

impl OperationPolicy<EditionExt> for EditionPolicy {
    type LogId = u64;

    fn admit(&self, operation: &Operation<EditionExt>) -> Result<Admission<u64>, Reject> {
        check_edition_operation(operation, self.only_key.as_ref(), self.limits).map_err(
            |error| {
                let code = match error {
                    EditionError::WrongAuthor => "edition-wrong-author",
                    _ => "edition-refused",
                };
                Reject::new(code, error.to_string())
            },
        )?;
        let key = *operation.header.verifying_key.as_bytes();
        Ok(Admission::keep(StoreTarget::new(edition_topic(&key), 0)))
    }
}
