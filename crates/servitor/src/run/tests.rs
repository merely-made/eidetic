// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::{BodyRevision, Lifecycle, ResidentBinding, ResidentId, Subject, Trigger};

fn reducer(limits: RunLimits) -> RunReducer {
    RunReducer::new(RunHeader {
        id: RunId(7),
        started_at_ms: 10,
        limits,
        ticket: RunTicket {
            binding: ResidentBinding {
                id: ResidentId([1; 16]),
                subject: Subject::new([2; 32]),
                revision: BodyRevision([3; 32]),
                generation: 4,
                lifecycle: Lifecycle::Active,
            },
            trigger: Trigger::Manual,
            required: vec![],
        },
    })
    .unwrap()
}
fn limits() -> RunLimits {
    RunLimits {
        decisions: 10,
        tool_calls: 10,
        tokens: 100,
        elapsed_ms: 1000,
        consecutive_failures: 3,
    }
}
fn read() -> Effect {
    Effect {
        kind: EffectKind::ReadOnly,
        operation: 1,
    }
}
fn consequential() -> Effect {
    Effect {
        kind: EffectKind::NeedsReconciliation,
        operation: 2,
    }
}
fn use_one() -> Usage {
    Usage {
        decisions: 1,
        tool_calls: 1,
        tokens: 2,
        elapsed_ms: 5,
        consecutive_failures: 0,
    }
}
fn intent(correlation: Correlation, effect: Effect, at_ms: u64) -> RunEvent {
    RunEvent::IntentRecorded {
        correlation,
        effect,
        usage: use_one(),
        at_ms,
    }
}
fn result(correlation: Correlation, result: ResultKind, at_ms: u64) -> RunEvent {
    RunEvent::ResultRecorded {
        correlation,
        result,
        usage: use_one(),
        at_ms,
    }
}

struct FakeProvider {
    calls: u64,
}
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
    for event in [intent_event, result_event] {
        replay.apply(event).unwrap();
    }
    assert_eq!(provider.calls, 1, "replay never calls a provider");
    assert_eq!(live, replay);
    assert_eq!(
        live.phase(),
        RunPhase::Ready {
            correlation: Correlation {
                step: 1,
                attempt: 0,
                ..first
            }
        }
    );
    assert_eq!(live.event_sequence(), 2);
    assert_eq!(
        live.usage(),
        Usage {
            decisions: 2,
            tool_calls: 2,
            tokens: 4,
            elapsed_ms: 10,
            consecutive_failures: 0
        }
    );
}

#[test]
fn rejects_duplicate_stale_and_wrong_correlations() {
    let mut run = reducer(limits());
    let key = run.pending_correlation().unwrap();
    run.apply(intent(key, read(), 11)).unwrap();
    assert_eq!(
        run.apply(intent(key, read(), 12)),
        Err(RunError::NotAwaiting)
    );
    assert_eq!(
        run.apply(result(
            Correlation { step: 1, ..key },
            ResultKind::Progress,
            12
        )),
        Err(RunError::DuplicateOrStale)
    );
    assert_eq!(
        run.apply(result(
            Correlation { attempt: 1, ..key },
            ResultKind::Progress,
            12
        )),
        Err(RunError::DuplicateOrStale)
    );
    assert_eq!(
        run.apply(result(
            Correlation {
                run_id: RunId(8),
                ..key
            },
            ResultKind::Progress,
            12
        )),
        Err(RunError::WrongRun)
    );
    assert_eq!(
        run.apply(result(
            Correlation {
                generation: 5,
                ..key
            },
            ResultKind::Progress,
            12
        )),
        Err(RunError::WrongGeneration)
    );
    run.apply(result(key, ResultKind::Progress, 12)).unwrap();
    assert_eq!(
        run.apply(result(key, ResultKind::Progress, 13)),
        Err(RunError::NotAwaiting)
    );
}

#[test]
fn cancellation_rejects_late_results_and_preserves_unknown_effect() {
    let mut run = reducer(limits());
    let key = run.pending_correlation().unwrap();
    run.apply(intent(key, consequential(), 11)).unwrap();
    run.apply(RunEvent::CancellationRequested { at_ms: 12 })
        .unwrap();
    assert_eq!(
        run.phase(),
        RunPhase::Reconciling {
            correlation: key,
            effect: consequential()
        }
    );
    assert_eq!(
        run.apply(result(key, ResultKind::Completed, 13)),
        Err(RunError::Cancelled)
    );
    run.apply(RunEvent::ReconciliationResolved {
        correlation: key,
        result: ResultKind::Completed,
        usage: use_one(),
        at_ms: 14,
    })
    .unwrap();
    assert_eq!(run.phase(), RunPhase::Terminal(TerminalOutcome::Cancelled));
}

#[test]
fn interruption_requires_explicit_reconciliation_only_for_consequential_effects() {
    let mut run = reducer(limits());
    let key = run.pending_correlation().unwrap();
    run.apply(intent(key, consequential(), 11)).unwrap();
    run.apply(RunEvent::Interrupted { at_ms: 12 }).unwrap();
    assert_eq!(
        run.phase(),
        RunPhase::Reconciling {
            correlation: key,
            effect: consequential()
        }
    );
    let mut read_run = reducer(limits());
    let read_key = read_run.pending_correlation().unwrap();
    read_run.apply(intent(read_key, read(), 11)).unwrap();
    read_run.apply(RunEvent::Interrupted { at_ms: 12 }).unwrap();
    assert_eq!(read_run.pending_correlation().unwrap().attempt, 1);
}

#[test]
fn exhausted_budget_is_terminal_after_a_nonterminal_result() {
    let mut run = reducer(RunLimits {
        decisions: 1,
        ..limits()
    });
    let key = run.pending_correlation().unwrap();
    run.apply(intent(key, read(), 11)).unwrap();
    run.apply(result(key, ResultKind::Progress, 12)).unwrap();
    assert_eq!(
        run.phase(),
        RunPhase::Terminal(TerminalOutcome::BudgetExhausted)
    );
}

#[test]
fn budget_does_not_discard_an_awaiting_consequential_effect() {
    let mut run = reducer(RunLimits {
        decisions: 1,
        ..limits()
    });
    let key = run.pending_correlation().unwrap();
    run.apply(intent(key, consequential(), 11)).unwrap();
    assert_eq!(
        run.phase(),
        RunPhase::Awaiting {
            correlation: key,
            effect: consequential()
        }
    );
    run.apply(RunEvent::Interrupted { at_ms: 12 }).unwrap();
    assert_eq!(
        run.phase(),
        RunPhase::Reconciling {
            correlation: key,
            effect: consequential()
        }
    );
    run.apply(RunEvent::ReconciliationResolved {
        correlation: key,
        result: ResultKind::Completed,
        usage: Usage::default(),
        at_ms: 13,
    })
    .unwrap();
    assert_eq!(run.phase(), RunPhase::Terminal(TerminalOutcome::Completed));
}

#[test]
fn rejected_event_is_atomic_even_after_a_charge_would_overflow() {
    let mut run = reducer(RunLimits {
        decisions: u64::MAX,
        ..limits()
    });
    let key = run.pending_correlation().unwrap();
    let before = run.clone();
    assert_eq!(
        run.apply(RunEvent::IntentRecorded {
            correlation: key,
            effect: read(),
            usage: Usage {
                decisions: u64::MAX,
                ..Usage::default()
            },
            at_ms: 11
        }),
        Ok(())
    );
    let charged = run.clone();
    assert_eq!(
        run.apply(result(key, ResultKind::Progress, 12)),
        Err(RunError::ArithmeticOverflow)
    );
    assert_eq!(run, charged);
    assert_ne!(run, before);
}

#[test]
fn zero_token_script_can_run_but_oversized_or_zero_cost_intents_cannot() {
    let mut scripted = reducer(RunLimits {
        tokens: 0,
        tool_calls: 0,
        decisions: 2,
        elapsed_ms: 20,
        consecutive_failures: 1,
    });
    let key = scripted.pending_correlation().unwrap();
    let scripted_usage = Usage {
        decisions: 1,
        elapsed_ms: 1,
        ..Usage::default()
    };
    scripted
        .apply(RunEvent::IntentRecorded {
            correlation: key,
            effect: read(),
            usage: scripted_usage,
            at_ms: 11,
        })
        .unwrap();
    scripted
        .apply(RunEvent::ResultRecorded {
            correlation: key,
            result: ResultKind::Progress,
            usage: Usage::default(),
            at_ms: 12,
        })
        .unwrap();
    assert!(matches!(scripted.phase(), RunPhase::Ready { .. }));
    let mut bounded = reducer(limits());
    let key = bounded.pending_correlation().unwrap();
    assert_eq!(
        bounded.apply(RunEvent::IntentRecorded {
            correlation: key,
            effect: read(),
            usage: Usage {
                tokens: 101,
                decisions: 1,
                ..Usage::default()
            },
            at_ms: 11
        }),
        Err(RunError::BudgetExceeded)
    );
    assert_eq!(
        bounded.apply(RunEvent::IntentRecorded {
            correlation: key,
            effect: read(),
            usage: Usage::default(),
            at_ms: 11
        }),
        Err(RunError::InvalidIntentUsage)
    );
}

#[test]
fn successful_progress_resets_the_consecutive_failure_counter() {
    let mut run = reducer(limits());
    let key = run.pending_correlation().unwrap();
    run.apply(intent(key, read(), 11)).unwrap();
    run.apply(RunEvent::ResultRecorded {
        correlation: key,
        result: ResultKind::RetryableFailure,
        usage: Usage {
            consecutive_failures: 1,
            ..use_one()
        },
        at_ms: 12,
    })
    .unwrap();
    assert_eq!(run.usage().consecutive_failures, 1);
    let retry = run.pending_correlation().unwrap();
    run.apply(intent(retry, read(), 13)).unwrap();
    run.apply(result(retry, ResultKind::Progress, 14)).unwrap();
    assert_eq!(run.usage().consecutive_failures, 0);
}

#[test]
fn zero_failure_budget_allows_successful_progress() {
    let mut run = reducer(RunLimits {
        decisions: 3,
        consecutive_failures: 0,
        ..limits()
    });
    let key = run.pending_correlation().unwrap();
    run.apply(intent(key, read(), 11)).unwrap();
    run.apply(result(key, ResultKind::Progress, 12)).unwrap();
    assert!(matches!(run.phase(), RunPhase::Ready { .. }));
    assert_eq!(run.usage().consecutive_failures, 0);
}

#[test]
fn success_clears_failures_before_the_budget_decision() {
    let mut run = reducer(RunLimits {
        decisions: 3,
        consecutive_failures: 2,
        ..limits()
    });
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
        usage: Usage {
            consecutive_failures: 1,
            ..use_one()
        },
        at_ms: 12,
    })
    .unwrap();
    assert_eq!(run.phase(), RunPhase::Terminal(TerminalOutcome::Failed));
    assert_eq!(run.usage().consecutive_failures, 1);
}
