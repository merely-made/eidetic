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
    CancellationRequested { at_ms: u64 },
    Paused { at_ms: u64 },
    Resumed { at_ms: u64 },
    /// Records that the host lost an in-flight request before its receipt.
    Interrupted { at_ms: u64 },
    /// Records that a ready or paused run cannot start more work within its
    /// remaining host budget. It cannot erase an outstanding effect.
    BudgetExhausted { at_ms: u64 },
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
    Ready { correlation: Correlation },
    Awaiting { correlation: Correlation, effect: Effect },
    Paused { correlation: Correlation },
    Reconciling { correlation: Correlation, effect: Effect },
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

impl RunReducer {
    /// Starts from an already admitted ticket. The ticket generation is the
    /// only accepted generation for all effects in this run.
    pub fn new(header: RunHeader) -> Result<Self, RunError> {
        let correlation = Correlation {
            run_id: header.id,
            step: 0,
            attempt: 0,
            generation: header.ticket.binding.generation,
        };
        Ok(Self {
            last_at_ms: header.started_at_ms,
            header,
            phase: RunPhase::Ready { correlation },
            usage: Usage::default(),
            event_sequence: 0,
            cancellation_requested: false,
        })
    }

    pub fn header(&self) -> &RunHeader { &self.header }
    pub fn ticket(&self) -> &RunTicket { &self.header.ticket }
    pub fn phase(&self) -> RunPhase { self.phase }
    pub fn usage(&self) -> Usage { self.usage }
    pub fn event_sequence(&self) -> u64 { self.event_sequence }
    /// Last recorded logical time, for hosts that clamp a regressed wall clock.
    pub fn last_at_ms(&self) -> u64 { self.last_at_ms }
    pub fn cancellation_requested(&self) -> bool { self.cancellation_requested }

    /// The pending correlation is observation for a host journal, never a
    /// dispatch command. It is `None` once terminal.
    pub fn pending_correlation(&self) -> Option<Correlation> {
        match self.phase {
            RunPhase::Ready { correlation }
            | RunPhase::Awaiting { correlation, .. }
            | RunPhase::Paused { correlation }
            | RunPhase::Reconciling { correlation, .. } => Some(correlation),
            RunPhase::Terminal(_) => None,
        }
    }

    /// Applies one already-recorded event with checked counters. No provider,
    /// command, retry, or wall clock is consulted here.
    pub fn apply(&mut self, event: RunEvent) -> Result<(), RunError> {
        // An invalid persisted event must not leave a partly charged reducer.
        let mut next = self.clone();
        next.apply_inner(event)?;
        *self = next;
        Ok(())
    }

    fn apply_inner(&mut self, event: RunEvent) -> Result<(), RunError> {
        let at_ms = event_at(event);
        if at_ms < self.last_at_ms { return Err(RunError::TimeWentBackwards); }
        match event {
            RunEvent::IntentRecorded { correlation, effect, usage, .. } => {
                self.require_ready(correlation)?;
                if usage.decisions == 0 || usage.consecutive_failures != 0 {
                    return Err(RunError::InvalidIntentUsage);
                }
                if self.would_exceed(usage)? { return Err(RunError::BudgetExceeded); }
                self.charge(usage)?;
                self.phase = RunPhase::Awaiting { correlation, effect };
            }
            RunEvent::ResultRecorded { correlation, result, usage, .. } => {
                let effect = self.require_awaiting(correlation)?;
                self.validate_result_usage(result, usage)?;
                self.charge(usage)?;
                self.apply_result(correlation, effect, result)?;
            }
            RunEvent::CancellationRequested { .. } => self.cancel()?,
            RunEvent::Paused { .. } => {
                if self.cancellation_requested { return Err(RunError::Cancelled); }
                let correlation = match self.phase {
                    RunPhase::Ready { correlation } => correlation,
                    RunPhase::Awaiting { .. } | RunPhase::Reconciling { .. } => return Err(RunError::NotAwaiting),
                    RunPhase::Paused { .. } => return Err(RunError::DuplicateOrStale),
                    RunPhase::Terminal(_) => return Err(RunError::Terminal),
                };
                self.phase = RunPhase::Paused { correlation };
            }
            RunEvent::Resumed { .. } => {
                if self.cancellation_requested { return Err(RunError::Cancelled); }
                let correlation = match self.phase {
                    RunPhase::Paused { correlation } => correlation,
                    RunPhase::Terminal(_) => return Err(RunError::Terminal),
                    _ => return Err(RunError::NotPaused),
                };
                self.phase = RunPhase::Ready { correlation };
            }
            RunEvent::Interrupted { .. } => self.interrupt()?,
            RunEvent::BudgetExhausted { .. } => match self.phase {
                RunPhase::Ready { .. } | RunPhase::Paused { .. } => {
                    self.phase = RunPhase::Terminal(TerminalOutcome::BudgetExhausted);
                }
                RunPhase::Terminal(_) => return Err(RunError::Terminal),
                _ => return Err(RunError::NotAwaiting),
            },
            RunEvent::ReconciliationResolved { correlation, result, usage, .. } => {
                let effect = self.require_reconciling(correlation)?;
                self.validate_result_usage(result, usage)?;
                self.charge(usage)?;
                self.apply_result(correlation, effect, result)?;
            }
        }
        self.last_at_ms = at_ms;
        self.event_sequence = self.event_sequence.checked_add(1).ok_or(RunError::ArithmeticOverflow)?;
        Ok(())
    }

    fn require_ready(&self, correlation: Correlation) -> Result<(), RunError> {
        self.check_correlation(correlation)?;
        if self.cancellation_requested { return Err(RunError::Cancelled); }
        match self.phase {
            RunPhase::Ready { correlation: expected } if expected == correlation => Ok(()),
            RunPhase::Ready { .. } => Err(RunError::DuplicateOrStale),
            RunPhase::Terminal(_) => Err(RunError::Terminal),
            _ => Err(RunError::NotAwaiting),
        }
    }

    fn require_awaiting(&self, correlation: Correlation) -> Result<Effect, RunError> {
        self.check_correlation(correlation)?;
        if self.cancellation_requested { return Err(RunError::Cancelled); }
        match self.phase {
            RunPhase::Awaiting { correlation: expected, effect } if expected == correlation => Ok(effect),
            RunPhase::Awaiting { .. } => Err(RunError::DuplicateOrStale),
            RunPhase::Terminal(_) => Err(RunError::Terminal),
            _ => Err(RunError::NotAwaiting),
        }
    }

    fn require_reconciling(&self, correlation: Correlation) -> Result<Effect, RunError> {
        self.check_correlation(correlation)?;
        match self.phase {
            RunPhase::Reconciling { correlation: expected, effect } if expected == correlation => Ok(effect),
            RunPhase::Reconciling { .. } => Err(RunError::DuplicateOrStale),
            RunPhase::Terminal(_) => Err(RunError::Terminal),
            _ => Err(RunError::NotReconciling),
        }
    }

    fn check_correlation(&self, correlation: Correlation) -> Result<(), RunError> {
        if correlation.run_id != self.header.id { return Err(RunError::WrongRun); }
        if correlation.generation != self.header.ticket.binding.generation { return Err(RunError::WrongGeneration); }
        Ok(())
    }

    fn charge(&mut self, added: Usage) -> Result<(), RunError> {
        self.usage = Usage {
            decisions: self.usage.decisions.checked_add(added.decisions).ok_or(RunError::ArithmeticOverflow)?,
            tool_calls: self.usage.tool_calls.checked_add(added.tool_calls).ok_or(RunError::ArithmeticOverflow)?,
            tokens: self.usage.tokens.checked_add(added.tokens).ok_or(RunError::ArithmeticOverflow)?,
            elapsed_ms: self.usage.elapsed_ms.checked_add(added.elapsed_ms).ok_or(RunError::ArithmeticOverflow)?,
            consecutive_failures: self.usage.consecutive_failures.checked_add(added.consecutive_failures).ok_or(RunError::ArithmeticOverflow)?,
        };
        Ok(())
    }

    fn would_exceed(&self, added: Usage) -> Result<bool, RunError> {
        Ok(self.usage.decisions.checked_add(added.decisions).ok_or(RunError::ArithmeticOverflow)? > self.header.limits.decisions
            || self.usage.tool_calls.checked_add(added.tool_calls).ok_or(RunError::ArithmeticOverflow)? > self.header.limits.tool_calls
            || self.usage.tokens.checked_add(added.tokens).ok_or(RunError::ArithmeticOverflow)? > self.header.limits.tokens
            || self.usage.elapsed_ms.checked_add(added.elapsed_ms).ok_or(RunError::ArithmeticOverflow)? > self.header.limits.elapsed_ms
            || self.usage.consecutive_failures.checked_add(added.consecutive_failures).ok_or(RunError::ArithmeticOverflow)? > self.header.limits.consecutive_failures)
    }

    fn validate_result_usage(&self, result: ResultKind, usage: Usage) -> Result<(), RunError> {
        match result {
            ResultKind::RetryableFailure | ResultKind::Failed if usage.consecutive_failures == 1 => Ok(()),
            ResultKind::RetryableFailure | ResultKind::Failed => Err(RunError::InvalidResultUsage),
            _ if usage.consecutive_failures == 0 => Ok(()),
            _ => Err(RunError::InvalidResultUsage),
        }
    }

    fn apply_result(&mut self, correlation: Correlation, effect: Effect, result: ResultKind) -> Result<(), RunError> {
        if !matches!(result, ResultKind::RetryableFailure | ResultKind::Failed) {
            self.usage.consecutive_failures = 0;
        }
        if self.cancellation_requested {
            self.phase = RunPhase::Terminal(TerminalOutcome::Cancelled);
            return Ok(());
        }
        self.phase = match result {
            ResultKind::Completed => RunPhase::Terminal(TerminalOutcome::Completed),
            ResultKind::Refused => RunPhase::Terminal(TerminalOutcome::Refused),
            ResultKind::Failed => RunPhase::Terminal(TerminalOutcome::Failed),
            ResultKind::Progress if self.exhausted() => RunPhase::Terminal(TerminalOutcome::BudgetExhausted),
            ResultKind::RetryableFailure if self.exhausted() => RunPhase::Terminal(TerminalOutcome::BudgetExhausted),
            ResultKind::Progress => RunPhase::Ready { correlation: next_step(correlation)? },
            ResultKind::RetryableFailure => RunPhase::Ready { correlation: next_attempt(correlation)? },
        };
        let _ = effect;
        Ok(())
    }

    fn exhausted(&self) -> bool {
        self.usage.decisions >= self.header.limits.decisions
            || (self.usage.consecutive_failures > 0
                && self.usage.consecutive_failures >= self.header.limits.consecutive_failures)
    }

    fn cancel(&mut self) -> Result<(), RunError> {
        if self.cancellation_requested { return Err(RunError::DuplicateOrStale); }
        match self.phase {
            RunPhase::Terminal(_) => return Err(RunError::Terminal),
            RunPhase::Awaiting { correlation, effect } if effect.kind == EffectKind::NeedsReconciliation => {
                self.cancellation_requested = true;
                self.phase = RunPhase::Reconciling { correlation, effect };
            }
            RunPhase::Awaiting { .. } | RunPhase::Ready { .. } | RunPhase::Paused { .. } => {
                self.cancellation_requested = true;
                self.phase = RunPhase::Terminal(TerminalOutcome::Cancelled);
            }
            RunPhase::Reconciling { .. } => self.cancellation_requested = true,
        }
        Ok(())
    }

    fn interrupt(&mut self) -> Result<(), RunError> {
        if self.cancellation_requested { return Err(RunError::Cancelled); }
        match self.phase {
            RunPhase::Awaiting { correlation, effect } if effect.kind == EffectKind::NeedsReconciliation => {
                self.phase = RunPhase::Reconciling { correlation, effect };
            }
            RunPhase::Awaiting { correlation, effect: Effect { kind: EffectKind::ReadOnly, .. } } => {
                self.phase = if self.exhausted() {
                    RunPhase::Terminal(TerminalOutcome::BudgetExhausted)
                } else {
                    RunPhase::Ready { correlation: next_attempt(correlation)? }
                };
            }
            RunPhase::Terminal(_) => return Err(RunError::Terminal),
            _ => return Err(RunError::NotAwaiting),
        }
        Ok(())
    }
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
    correlation.step = correlation.step.checked_add(1).ok_or(RunError::ArithmeticOverflow)?;
    correlation.attempt = 0;
    Ok(correlation)
}

fn next_attempt(mut correlation: Correlation) -> Result<Correlation, RunError> {
    correlation.attempt = correlation.attempt.checked_add(1).ok_or(RunError::ArithmeticOverflow)?;
    Ok(correlation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BodyRevision, Lifecycle, ResidentBinding, ResidentId, Subject, Trigger};

    fn reducer(limits: RunLimits) -> RunReducer {
        RunReducer::new(RunHeader {
            id: RunId(7), started_at_ms: 10, limits,
            ticket: RunTicket { binding: ResidentBinding { id: ResidentId([1; 16]), subject: Subject::new([2; 32]), revision: BodyRevision([3; 32]), generation: 4, lifecycle: Lifecycle::Active }, trigger: Trigger::Manual, required: vec![] },
        }).unwrap()
    }
    fn limits() -> RunLimits { RunLimits { decisions: 10, tool_calls: 10, tokens: 100, elapsed_ms: 1000, consecutive_failures: 3 } }
    fn read() -> Effect { Effect { kind: EffectKind::ReadOnly, operation: 1 } }
    fn consequential() -> Effect { Effect { kind: EffectKind::NeedsReconciliation, operation: 2 } }
    fn use_one() -> Usage { Usage { decisions: 1, tool_calls: 1, tokens: 2, elapsed_ms: 5, consecutive_failures: 0 } }
    fn intent(correlation: Correlation, effect: Effect, at_ms: u64) -> RunEvent { RunEvent::IntentRecorded { correlation, effect, usage: use_one(), at_ms } }
    fn result(correlation: Correlation, result: ResultKind, at_ms: u64) -> RunEvent { RunEvent::ResultRecorded { correlation, result, usage: use_one(), at_ms } }

    struct FakeProvider { calls: u64 }
    impl FakeProvider {
        fn result_for(&mut self, _effect: Effect) -> ResultKind {
            self.calls += 1;
            ResultKind::Progress
        }
    }

    #[test]
    fn fake_provider_replays_same_deterministic_state() {
        let mut live = reducer(limits());
        let first = live.pending_correlation().unwrap();
        let intent_event = intent(first, read(), 11);
        live.apply(intent_event).unwrap();
        assert!(matches!(live.phase(), RunPhase::Awaiting { .. }));
        let mut provider = FakeProvider { calls: 0 };
        let result_event = result(first, provider.result_for(read()), 12);
        live.apply(result_event).unwrap();
        assert_eq!(provider.calls, 1);
        let mut replay = reducer(limits());
        for event in [intent_event, result_event] { replay.apply(event).unwrap(); }
        assert_eq!(provider.calls, 1, "replay never calls a provider");
        assert_eq!(live, replay);
        assert_eq!(live.phase(), RunPhase::Ready { correlation: Correlation { step: 1, attempt: 0, ..first } });
        assert_eq!(live.event_sequence(), 2);
        assert_eq!(live.usage(), Usage { decisions: 2, tool_calls: 2, tokens: 4, elapsed_ms: 10, consecutive_failures: 0 });
    }

    #[test]
    fn rejects_duplicate_stale_and_wrong_correlations() {
        let mut run = reducer(limits()); let key = run.pending_correlation().unwrap();
        run.apply(intent(key, read(), 11)).unwrap();
        assert_eq!(run.apply(intent(key, read(), 12)), Err(RunError::NotAwaiting));
        assert_eq!(run.apply(result(Correlation { step: 1, ..key }, ResultKind::Progress, 12)), Err(RunError::DuplicateOrStale));
        assert_eq!(run.apply(result(Correlation { attempt: 1, ..key }, ResultKind::Progress, 12)), Err(RunError::DuplicateOrStale));
        assert_eq!(run.apply(result(Correlation { run_id: RunId(8), ..key }, ResultKind::Progress, 12)), Err(RunError::WrongRun));
        assert_eq!(run.apply(result(Correlation { generation: 5, ..key }, ResultKind::Progress, 12)), Err(RunError::WrongGeneration));
        run.apply(result(key, ResultKind::Progress, 12)).unwrap();
        assert_eq!(run.apply(result(key, ResultKind::Progress, 13)), Err(RunError::NotAwaiting));
    }

    #[test]
    fn cancellation_rejects_late_results_and_preserves_unknown_effect() {
        let mut run = reducer(limits()); let key = run.pending_correlation().unwrap();
        run.apply(intent(key, consequential(), 11)).unwrap();
        run.apply(RunEvent::CancellationRequested { at_ms: 12 }).unwrap();
        assert_eq!(run.phase(), RunPhase::Reconciling { correlation: key, effect: consequential() });
        assert_eq!(run.apply(result(key, ResultKind::Completed, 13)), Err(RunError::Cancelled));
        run.apply(RunEvent::ReconciliationResolved { correlation: key, result: ResultKind::Completed, usage: use_one(), at_ms: 14 }).unwrap();
        assert_eq!(run.phase(), RunPhase::Terminal(TerminalOutcome::Cancelled));
    }

    #[test]
    fn interruption_requires_explicit_reconciliation_only_for_consequential_effects() {
        let mut run = reducer(limits()); let key = run.pending_correlation().unwrap();
        run.apply(intent(key, consequential(), 11)).unwrap(); run.apply(RunEvent::Interrupted { at_ms: 12 }).unwrap();
        assert_eq!(run.phase(), RunPhase::Reconciling { correlation: key, effect: consequential() });
        let mut read_run = reducer(limits()); let read_key = read_run.pending_correlation().unwrap();
        read_run.apply(intent(read_key, read(), 11)).unwrap(); read_run.apply(RunEvent::Interrupted { at_ms: 12 }).unwrap();
        assert_eq!(read_run.pending_correlation().unwrap().attempt, 1);
    }

    #[test]
    fn exhausted_budget_is_terminal_after_a_nonterminal_result() {
        let mut run = reducer(RunLimits { decisions: 1, ..limits() }); let key = run.pending_correlation().unwrap();
        run.apply(intent(key, read(), 11)).unwrap(); run.apply(result(key, ResultKind::Progress, 12)).unwrap();
        assert_eq!(run.phase(), RunPhase::Terminal(TerminalOutcome::BudgetExhausted));
    }

    #[test]
    fn budget_does_not_discard_an_awaiting_consequential_effect() {
        let mut run = reducer(RunLimits { decisions: 1, ..limits() });
        let key = run.pending_correlation().unwrap();
        run.apply(intent(key, consequential(), 11)).unwrap();
        assert_eq!(run.phase(), RunPhase::Awaiting { correlation: key, effect: consequential() });
        run.apply(RunEvent::Interrupted { at_ms: 12 }).unwrap();
        assert_eq!(run.phase(), RunPhase::Reconciling { correlation: key, effect: consequential() });
        run.apply(RunEvent::ReconciliationResolved { correlation: key, result: ResultKind::Completed, usage: Usage::default(), at_ms: 13 }).unwrap();
        assert_eq!(run.phase(), RunPhase::Terminal(TerminalOutcome::Completed));
    }

    #[test]
    fn rejected_event_is_atomic_even_after_a_charge_would_overflow() {
        let mut run = reducer(RunLimits { decisions: u64::MAX, ..limits() });
        let key = run.pending_correlation().unwrap();
        let before = run.clone();
        assert_eq!(run.apply(RunEvent::IntentRecorded { correlation: key, effect: read(), usage: Usage { decisions: u64::MAX, ..Usage::default() }, at_ms: 11 }), Ok(()));
        let charged = run.clone();
        assert_eq!(run.apply(result(key, ResultKind::Progress, 12)), Err(RunError::ArithmeticOverflow));
        assert_eq!(run, charged);
        assert_ne!(run, before);
    }

    #[test]
    fn zero_token_script_can_run_but_oversized_or_zero_cost_intents_cannot() {
        let mut scripted = reducer(RunLimits { tokens: 0, tool_calls: 0, decisions: 2, elapsed_ms: 20, consecutive_failures: 1 });
        let key = scripted.pending_correlation().unwrap();
        let scripted_usage = Usage { decisions: 1, elapsed_ms: 1, ..Usage::default() };
        scripted.apply(RunEvent::IntentRecorded { correlation: key, effect: read(), usage: scripted_usage, at_ms: 11 }).unwrap();
        scripted.apply(RunEvent::ResultRecorded { correlation: key, result: ResultKind::Progress, usage: Usage::default(), at_ms: 12 }).unwrap();
        assert!(matches!(scripted.phase(), RunPhase::Ready { .. }));
        let mut bounded = reducer(limits()); let key = bounded.pending_correlation().unwrap();
        assert_eq!(bounded.apply(RunEvent::IntentRecorded { correlation: key, effect: read(), usage: Usage { tokens: 101, decisions: 1, ..Usage::default() }, at_ms: 11 }), Err(RunError::BudgetExceeded));
        assert_eq!(bounded.apply(RunEvent::IntentRecorded { correlation: key, effect: read(), usage: Usage::default(), at_ms: 11 }), Err(RunError::InvalidIntentUsage));
    }

    #[test]
    fn successful_progress_resets_the_consecutive_failure_counter() {
        let mut run = reducer(limits()); let key = run.pending_correlation().unwrap();
        run.apply(intent(key, read(), 11)).unwrap();
        run.apply(RunEvent::ResultRecorded { correlation: key, result: ResultKind::RetryableFailure, usage: Usage { consecutive_failures: 1, ..use_one() }, at_ms: 12 }).unwrap();
        assert_eq!(run.usage().consecutive_failures, 1);
        let retry = run.pending_correlation().unwrap(); run.apply(intent(retry, read(), 13)).unwrap();
        run.apply(result(retry, ResultKind::Progress, 14)).unwrap();
        assert_eq!(run.usage().consecutive_failures, 0);
    }

    #[test]
    fn zero_failure_budget_allows_successful_progress() {
        let mut run = reducer(RunLimits { decisions: 3, consecutive_failures: 0, ..limits() });
        let key = run.pending_correlation().unwrap();
        run.apply(intent(key, read(), 11)).unwrap();
        run.apply(result(key, ResultKind::Progress, 12)).unwrap();
        assert!(matches!(run.phase(), RunPhase::Ready { .. }));
        assert_eq!(run.usage().consecutive_failures, 0);
    }

    #[test]
    fn success_clears_failures_before_the_budget_decision() {
        let mut run = reducer(RunLimits { decisions: 3, consecutive_failures: 2, ..limits() });
        run.usage.consecutive_failures = 2;
        let key = run.pending_correlation().unwrap();
        run.apply(intent(key, read(), 11)).unwrap();
        run.apply(result(key, ResultKind::Progress, 12)).unwrap();
        assert!(matches!(run.phase(), RunPhase::Ready { .. }));
        assert_eq!(run.usage().consecutive_failures, 0);
    }

    #[test]
    fn failed_terminal_retains_its_failure_observation() {
        let mut run = reducer(limits());
        let key = run.pending_correlation().unwrap();
        run.apply(intent(key, read(), 11)).unwrap();
        run.apply(RunEvent::ResultRecorded {
            correlation: key,
            result: ResultKind::Failed,
            usage: Usage { consecutive_failures: 1, ..use_one() },
            at_ms: 12,
        }).unwrap();
        assert_eq!(run.phase(), RunPhase::Terminal(TerminalOutcome::Failed));
        assert_eq!(run.usage().consecutive_failures, 1);
    }
}
