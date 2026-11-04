// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

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

    pub fn header(&self) -> &RunHeader {
        &self.header
    }
    pub fn ticket(&self) -> &RunTicket {
        &self.header.ticket
    }
    pub fn phase(&self) -> RunPhase {
        self.phase
    }
    pub fn usage(&self) -> Usage {
        self.usage
    }
    pub fn event_sequence(&self) -> u64 {
        self.event_sequence
    }
    /// Last recorded logical time, for hosts that clamp a regressed wall clock.
    pub fn last_at_ms(&self) -> u64 {
        self.last_at_ms
    }
    pub fn cancellation_requested(&self) -> bool {
        self.cancellation_requested
    }

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

    pub(super) fn apply_inner(&mut self, event: RunEvent) -> Result<(), RunError> {
        let at_ms = event_at(event);
        if at_ms < self.last_at_ms {
            return Err(RunError::TimeWentBackwards);
        }
        match event {
            RunEvent::IntentRecorded {
                correlation,
                effect,
                usage,
                ..
            } => {
                self.require_ready(correlation)?;
                if usage.decisions == 0 || usage.consecutive_failures != 0 {
                    return Err(RunError::InvalidIntentUsage);
                }
                if self.would_exceed(usage)? {
                    return Err(RunError::BudgetExceeded);
                }
                self.charge(usage)?;
                self.phase = RunPhase::Awaiting {
                    correlation,
                    effect,
                };
            }
            RunEvent::ResultRecorded {
                correlation,
                result,
                usage,
                ..
            } => {
                let effect = self.require_awaiting(correlation)?;
                self.validate_result_usage(result, usage)?;
                self.charge(usage)?;
                self.apply_result(correlation, effect, result)?;
            }
            RunEvent::CancellationRequested { .. } => self.cancel()?,
            RunEvent::Paused { .. } => {
                if self.cancellation_requested {
                    return Err(RunError::Cancelled);
                }
                let correlation = match self.phase {
                    RunPhase::Ready { correlation } => correlation,
                    RunPhase::Awaiting { .. } | RunPhase::Reconciling { .. } => {
                        return Err(RunError::NotAwaiting);
                    }
                    RunPhase::Paused { .. } => return Err(RunError::DuplicateOrStale),
                    RunPhase::Terminal(_) => return Err(RunError::Terminal),
                };
                self.phase = RunPhase::Paused { correlation };
            }
            RunEvent::Resumed { .. } => {
                if self.cancellation_requested {
                    return Err(RunError::Cancelled);
                }
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
            RunEvent::ReconciliationResolved {
                correlation,
                result,
                usage,
                ..
            } => {
                let effect = self.require_reconciling(correlation)?;
                self.validate_result_usage(result, usage)?;
                self.charge(usage)?;
                self.apply_result(correlation, effect, result)?;
            }
        }
        self.last_at_ms = at_ms;
        self.event_sequence = self
            .event_sequence
            .checked_add(1)
            .ok_or(RunError::ArithmeticOverflow)?;
        Ok(())
    }

    pub(super) fn require_ready(&self, correlation: Correlation) -> Result<(), RunError> {
        self.check_correlation(correlation)?;
        if self.cancellation_requested {
            return Err(RunError::Cancelled);
        }
        match self.phase {
            RunPhase::Ready {
                correlation: expected,
            } if expected == correlation => Ok(()),
            RunPhase::Ready { .. } => Err(RunError::DuplicateOrStale),
            RunPhase::Terminal(_) => Err(RunError::Terminal),
            _ => Err(RunError::NotAwaiting),
        }
    }

    pub(super) fn require_awaiting(&self, correlation: Correlation) -> Result<Effect, RunError> {
        self.check_correlation(correlation)?;
        if self.cancellation_requested {
            return Err(RunError::Cancelled);
        }
        match self.phase {
            RunPhase::Awaiting {
                correlation: expected,
                effect,
            } if expected == correlation => Ok(effect),
            RunPhase::Awaiting { .. } => Err(RunError::DuplicateOrStale),
            RunPhase::Terminal(_) => Err(RunError::Terminal),
            _ => Err(RunError::NotAwaiting),
        }
    }

    pub(super) fn require_reconciling(&self, correlation: Correlation) -> Result<Effect, RunError> {
        self.check_correlation(correlation)?;
        match self.phase {
            RunPhase::Reconciling {
                correlation: expected,
                effect,
            } if expected == correlation => Ok(effect),
            RunPhase::Reconciling { .. } => Err(RunError::DuplicateOrStale),
            RunPhase::Terminal(_) => Err(RunError::Terminal),
            _ => Err(RunError::NotReconciling),
        }
    }

    pub(super) fn check_correlation(&self, correlation: Correlation) -> Result<(), RunError> {
        if correlation.run_id != self.header.id {
            return Err(RunError::WrongRun);
        }
        if correlation.generation != self.header.ticket.binding.generation {
            return Err(RunError::WrongGeneration);
        }
        Ok(())
    }

    pub(super) fn charge(&mut self, added: Usage) -> Result<(), RunError> {
        self.usage = Usage {
            decisions: self
                .usage
                .decisions
                .checked_add(added.decisions)
                .ok_or(RunError::ArithmeticOverflow)?,
            tool_calls: self
                .usage
                .tool_calls
                .checked_add(added.tool_calls)
                .ok_or(RunError::ArithmeticOverflow)?,
            tokens: self
                .usage
                .tokens
                .checked_add(added.tokens)
                .ok_or(RunError::ArithmeticOverflow)?,
            elapsed_ms: self
                .usage
                .elapsed_ms
                .checked_add(added.elapsed_ms)
                .ok_or(RunError::ArithmeticOverflow)?,
            consecutive_failures: self
                .usage
                .consecutive_failures
                .checked_add(added.consecutive_failures)
                .ok_or(RunError::ArithmeticOverflow)?,
        };
        Ok(())
    }

    pub(super) fn would_exceed(&self, added: Usage) -> Result<bool, RunError> {
        Ok(self
            .usage
            .decisions
            .checked_add(added.decisions)
            .ok_or(RunError::ArithmeticOverflow)?
            > self.header.limits.decisions
            || self
                .usage
                .tool_calls
                .checked_add(added.tool_calls)
                .ok_or(RunError::ArithmeticOverflow)?
                > self.header.limits.tool_calls
            || self
                .usage
                .tokens
                .checked_add(added.tokens)
                .ok_or(RunError::ArithmeticOverflow)?
                > self.header.limits.tokens
            || self
                .usage
                .elapsed_ms
                .checked_add(added.elapsed_ms)
                .ok_or(RunError::ArithmeticOverflow)?
                > self.header.limits.elapsed_ms
            || self
                .usage
                .consecutive_failures
                .checked_add(added.consecutive_failures)
                .ok_or(RunError::ArithmeticOverflow)?
                > self.header.limits.consecutive_failures)
    }

    pub(super) fn validate_result_usage(
        &self,
        result: ResultKind,
        usage: Usage,
    ) -> Result<(), RunError> {
        match result {
            ResultKind::RetryableFailure | ResultKind::Failed
                if usage.consecutive_failures == 1 =>
            {
                Ok(())
            }
            ResultKind::RetryableFailure | ResultKind::Failed => Err(RunError::InvalidResultUsage),
            _ if usage.consecutive_failures == 0 => Ok(()),
            _ => Err(RunError::InvalidResultUsage),
        }
    }

    pub(super) fn apply_result(
        &mut self,
        correlation: Correlation,
        effect: Effect,
        result: ResultKind,
    ) -> Result<(), RunError> {
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
            ResultKind::Progress if self.exhausted() => {
                RunPhase::Terminal(TerminalOutcome::BudgetExhausted)
            }
            ResultKind::RetryableFailure if self.exhausted() => {
                RunPhase::Terminal(TerminalOutcome::BudgetExhausted)
            }
            ResultKind::Progress => RunPhase::Ready {
                correlation: next_step(correlation)?,
            },
            ResultKind::RetryableFailure => RunPhase::Ready {
                correlation: next_attempt(correlation)?,
            },
        };
        let _ = effect;
        Ok(())
    }

    pub(super) fn exhausted(&self) -> bool {
        self.usage.decisions >= self.header.limits.decisions
            || (self.usage.consecutive_failures > 0
                && self.usage.consecutive_failures >= self.header.limits.consecutive_failures)
    }

    pub(super) fn cancel(&mut self) -> Result<(), RunError> {
        if self.cancellation_requested {
            return Err(RunError::DuplicateOrStale);
        }
        match self.phase {
            RunPhase::Terminal(_) => return Err(RunError::Terminal),
            RunPhase::Awaiting {
                correlation,
                effect,
            } if effect.kind == EffectKind::NeedsReconciliation => {
                self.cancellation_requested = true;
                self.phase = RunPhase::Reconciling {
                    correlation,
                    effect,
                };
            }
            RunPhase::Awaiting { .. } | RunPhase::Ready { .. } | RunPhase::Paused { .. } => {
                self.cancellation_requested = true;
                self.phase = RunPhase::Terminal(TerminalOutcome::Cancelled);
            }
            RunPhase::Reconciling { .. } => self.cancellation_requested = true,
        }
        Ok(())
    }

    pub(super) fn interrupt(&mut self) -> Result<(), RunError> {
        if self.cancellation_requested {
            return Err(RunError::Cancelled);
        }
        match self.phase {
            RunPhase::Awaiting {
                correlation,
                effect,
            } if effect.kind == EffectKind::NeedsReconciliation => {
                self.phase = RunPhase::Reconciling {
                    correlation,
                    effect,
                };
            }
            RunPhase::Awaiting {
                correlation,
                effect:
                    Effect {
                        kind: EffectKind::ReadOnly,
                        ..
                    },
            } => {
                self.phase = if self.exhausted() {
                    RunPhase::Terminal(TerminalOutcome::BudgetExhausted)
                } else {
                    RunPhase::Ready {
                        correlation: next_attempt(correlation)?,
                    }
                };
            }
            RunPhase::Terminal(_) => return Err(RunError::Terminal),
            _ => return Err(RunError::NotAwaiting),
        }
        Ok(())
    }
}
