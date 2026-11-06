// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The capture error: a storage cause kept typed, a label chain kept readable.

use muniment::error::StoreError;

/// Why a capture, seal or verification step refused.
///
/// A storage failure stays a [`StoreError`] however many [`context`](Self::context)
/// labels wrap it, so a host can still tell a limit refusal from a semantic
/// one. `Display` renders `label: cause`, the shape Turnstone's stored tests
/// and receipts already match on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureError {
    /// The backend refused or failed.
    Storage(StoreError),
    /// A label over an inner cause.
    Context {
        /// What the failed step was doing.
        label: String,
        /// The refusal underneath.
        cause: Box<CaptureError>,
    },
    /// A semantic refusal (identity, custody, provenance, encoding).
    Other(String),
}

impl CaptureError {
    /// Wrap this error with the step that failed.
    #[must_use]
    pub fn context(self, label: &str) -> Self {
        Self::Context {
            label: label.into(),
            cause: Box::new(self),
        }
    }

    /// The backend failure underneath, if there is one.
    #[must_use]
    pub fn storage_error(&self) -> Option<&StoreError> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Context { cause, .. } => cause.storage_error(),
            Self::Other(_) => None,
        }
    }
}

impl From<String> for CaptureError {
    fn from(value: String) -> Self {
        Self::Other(value)
    }
}

impl From<&str> for CaptureError {
    fn from(value: &str) -> Self {
        Self::Other(value.into())
    }
}

impl From<StoreError> for CaptureError {
    fn from(value: StoreError) -> Self {
        Self::Storage(value)
    }
}

impl From<crate::Error> for CaptureError {
    fn from(value: crate::Error) -> Self {
        match value.storage_error() {
            Some(error) => Self::Storage(error.clone()),
            None => Self::Other(value.to_string()),
        }
    }
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(f),
            Self::Context { label, cause } => write!(f, "{label}: {cause}"),
            Self::Other(error) => f.write_str(error),
        }
    }
}

impl std::error::Error for CaptureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Context { cause, .. } => Some(cause.as_ref()),
            Self::Other(_) => None,
        }
    }
}
