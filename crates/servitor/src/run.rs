// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Deterministic, event-only reduction for one admitted resident run.
//!
//! This module never executes an effect. Hosts persist an intent before they
//! dispatch it, then persist a result (or an interruption) and replay those
//! records through [`RunReducer::apply`]. An interrupted consequential effect
//! is deliberately held in [`RunPhase::Reconciling`]; replay never retries it.

use crate::RunTicket;

/// Host-assigned identity for one invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RunId(pub u64);

/// Correlates an intent and its one result across a host boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Correlation {
    pub run_id: RunId,
    pub step: u64,
    pub attempt: u64,
    pub generation: u64,
}

/// All host-supplied limits for one run.
/// A zero `decisions` limit permits no intent. Other zero resource limits
/// permit work that does not charge that resource; zero allowed consecutive
/// failures prevents a retry after the first observed failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunLimits {
    pub decisions: u64,
    pub tool_calls: u64,
    pub tokens: u64,
    pub elapsed_ms: u64,
    pub consecutive_failures: u64,
}

/// Host-observed usage charged by a recorded event.
///
/// Every field is a delta, never an absolute counter. `consecutive_failures`
/// is one for a recorded failure observation and zero otherwise; the reducer
/// resets its running count after progress, completion or refusal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub decisions: u64,
    pub tool_calls: u64,
    pub tokens: u64,
    pub elapsed_ms: u64,
    pub consecutive_failures: u64,
}

/// Immutable starting facts for a reducer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunHeader {
    pub id: RunId,
    pub ticket: RunTicket,
    pub started_at_ms: u64,
    pub limits: RunLimits,
}

/// Whether an interrupted operation may safely return to ready state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectKind {
    ReadOnly,
    NeedsReconciliation,
}

/// A bounded host operation declaration. `operation` is a host descriptor,
/// while [`Correlation`] remains the unique pending-request identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    pub kind: EffectKind,
    pub operation: u64,
}

/// A host result. Progress and retryable failure leave a run ready for another
/// explicitly recorded intent; the other variants finish it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultKind {
    Progress,
    Completed,
    Refused,
    Failed,
    RetryableFailure,
}

/// Recorded input to the reducer. None of these variants dispatches work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunEvent {
    IntentRecorded {
        correlation: Correlation,
        effect: Effect,
        usage: Usage,
        at_ms: u64,
    },
    ResultRecorded {
        correlation: Correlation,
        result: ResultKind,
        usage: Usage,
        at_ms: u64,
    },
    CancellationRequested {
        at_ms: u64,
    },
    Paused {
        at_ms: u64,
    },
    Resumed {
        at_ms: u64,
    },
    /// Records that the host lost an in-flight request before its receipt.
    Interrupted {
        at_ms: u64,
    },
    /// Records that a ready or paused run cannot start more work within its
    /// remaining host budget. It cannot erase an outstanding effect.
    BudgetExhausted {
        at_ms: u64,
    },
    ReconciliationResolved {
        correlation: Correlation,
        result: ResultKind,
        usage: Usage,
        at_ms: u64,
    },
}

/// Observable state. `Ready` is only a state for a host decision; it is never
/// a request to execute anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunPhase {
    Ready {
        correlation: Correlation,
    },
    Awaiting {
        correlation: Correlation,
        effect: Effect,
    },
    Paused {
        correlation: Correlation,
    },
    Reconciling {
        correlation: Correlation,
        effect: Effect,
    },
    Terminal(TerminalOutcome),
}

/// The durable disposition of a finished run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalOutcome {
    Completed,
    Refused,
    Cancelled,
    Failed,
    BudgetExhausted,
}

/// Why an event cannot be reduced into the current state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunError {
    ArithmeticOverflow,
    TimeWentBackwards,
    WrongRun,
    WrongGeneration,
    DuplicateOrStale,
    NotAwaiting,
    NotReconciling,
    NotPaused,
    BudgetExceeded,
    InvalidIntentUsage,
    InvalidResultUsage,
    Cancelled,
    Terminal,
}

/// Pure state reducer for one run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunReducer {
    header: RunHeader,
    phase: RunPhase,
    usage: Usage,
    event_sequence: u64,
    last_at_ms: u64,
    cancellation_requested: bool,
}

fn event_at(event: RunEvent) -> u64 {
    match event {
        RunEvent::IntentRecorded { at_ms, .. }
        | RunEvent::ResultRecorded { at_ms, .. }
        | RunEvent::CancellationRequested { at_ms }
        | RunEvent::Paused { at_ms }
        | RunEvent::Resumed { at_ms }
        | RunEvent::Interrupted { at_ms }
        | RunEvent::BudgetExhausted { at_ms }
        | RunEvent::ReconciliationResolved { at_ms, .. } => at_ms,
    }
}

fn next_step(mut correlation: Correlation) -> Result<Correlation, RunError> {
    correlation.step = correlation
        .step
        .checked_add(1)
        .ok_or(RunError::ArithmeticOverflow)?;
    correlation.attempt = 0;
    Ok(correlation)
}

fn next_attempt(mut correlation: Correlation) -> Result<Correlation, RunError> {
    correlation.attempt = correlation
        .attempt
        .checked_add(1)
        .ok_or(RunError::ArithmeticOverflow)?;
    Ok(correlation)
}

#[cfg(test)]
mod tests;

mod reducer;
