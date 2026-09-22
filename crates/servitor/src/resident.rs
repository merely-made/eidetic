// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Portable resident identity and run admission.
//!
//! This module admits a proposed run against a caller-supplied authority
//! provider and returns a snapshot for the caller to revalidate. A ticket is
//! not a capability or dispatch permission: callers must gate every action
//! themselves. This layer does not persist tickets, replay effects, or own
//! execution.

use crate::{AuthorityProvider, Cap, Mode, Subject};

/// Stable identity for one resident instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResidentId(pub [u8; 16]);

/// Revision of the resident body bound to an instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyRevision(pub [u8; 32]);

/// Current lifecycle state of a resident binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    Active,
    Paused,
    Revoked,
}

/// The identity and body snapshot against which a resident run is admitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResidentBinding {
    /// Host-assigned identity for this resident instance.
    pub id: ResidentId,
    /// Subject whose authority is consulted for each admission.
    pub subject: Subject,
    /// Host-supplied body digest or revision identifier; this type does not
    /// hash, mint, or otherwise derive it.
    pub revision: BodyRevision,
    /// Host-owned monotonic binding generation.
    pub generation: u64,
    /// Current host-owned lifecycle state.
    pub lifecycle: Lifecycle,
}

/// Host-supplied reason for admitting a run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trigger {
    Manual,
    Journal { source: String, first: u64, last: u64 },
    Clock { at_ms: u64 },
}

/// Why a resident run could not be admitted or revalidated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    Paused,
    Revoked,
    StaleBinding,
    Unauthorized { cap: Cap, mode: Mode },
    InvalidTrigger,
}

/// A run admission snapshot. This is not a capability or dispatch permission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunTicket {
    /// The binding snapshot admitted by [`admit`].
    pub binding: ResidentBinding,
    /// The host-supplied trigger admitted by [`admit`].
    pub trigger: Trigger,
    /// Requirements captured at admission and always rechecked on revalidation.
    pub required: Vec<(Cap, Mode)>,
}

fn check_lifecycle(lifecycle: Lifecycle) -> Result<(), AdmissionError> {
    match lifecycle {
        Lifecycle::Active => Ok(()),
        Lifecycle::Paused => Err(AdmissionError::Paused),
        Lifecycle::Revoked => Err(AdmissionError::Revoked),
    }
}

fn valid_trigger(trigger: &Trigger) -> bool {
    match trigger {
        Trigger::Manual | Trigger::Clock { .. } => true,
        Trigger::Journal { source, first, last } => {
            !source.trim().is_empty() && first <= last
        }
    }
}

fn check_authority(
    binding: &ResidentBinding,
    authority: &impl AuthorityProvider,
    required: &[(Cap, Mode)],
) -> Result<(), AdmissionError> {
    for (cap, mode) in required {
        if !authority.covers(binding.subject, cap, *mode) {
            return Err(AdmissionError::Unauthorized {
                cap: cap.clone(),
                mode: *mode,
            });
        }
    }
    Ok(())
}

/// Admit a run from an active binding under the current explicit authority.
pub fn admit(
    binding: &ResidentBinding,
    trigger: Trigger,
    authority: &impl AuthorityProvider,
    required: &[(Cap, Mode)],
) -> Result<RunTicket, AdmissionError> {
    check_lifecycle(binding.lifecycle)?;
    if !valid_trigger(&trigger) {
        return Err(AdmissionError::InvalidTrigger);
    }
    check_authority(binding, authority, required)?;
    Ok(RunTicket {
        binding: binding.clone(),
        trigger,
        required: required.to_vec(),
    })
}

/// Revalidate a ticket against the current binding and authority.
///
/// This checks current facts and does not certify that a prior admission or
/// any action performed under it remains valid.
pub fn revalidate(
    ticket: &RunTicket,
    current: &ResidentBinding,
    authority: &impl AuthorityProvider,
    required: &[(Cap, Mode)],
) -> Result<(), AdmissionError> {
    check_lifecycle(current.lifecycle)?;
    if !valid_trigger(&ticket.trigger) {
        return Err(AdmissionError::InvalidTrigger);
    }
    if ticket.binding != *current {
        return Err(AdmissionError::StaleBinding);
    }
    check_authority(current, authority, &ticket.required)?;
    check_authority(current, authority, required)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Grant, GrantTable};

    fn binding(lifecycle: Lifecycle) -> ResidentBinding {
        ResidentBinding {
            id: ResidentId([1; 16]),
            subject: Subject::new([2; 32]),
            revision: BodyRevision([3; 32]),
            generation: 4,
            lifecycle,
        }
    }

    fn required() -> (Cap, Mode) {
        (Cap::scope("resident").unwrap(), Mode::Write)
    }

    fn authority(subject: Subject, cap: Cap, mode: Mode) -> GrantTable {
        GrantTable::new().with_grant(Grant::new(subject, cap, mode))
    }

    #[test]
    fn lifecycle_is_checked_before_authority() {
        let paused = binding(Lifecycle::Paused);
        let (cap, mode) = required();
        assert_eq!(
            admit(&paused, Trigger::Manual, &GrantTable::new(), &[(cap, mode)]),
            Err(AdmissionError::Paused)
        );

        let revoked = binding(Lifecycle::Revoked);
        let (cap, mode) = required();
        assert_eq!(
            admit(&revoked, Trigger::Manual, &GrantTable::new(), &[(cap, mode)]),
            Err(AdmissionError::Revoked)
        );
    }

    #[test]
    fn revalidation_catches_revocation_and_binding_changes() {
        let original = binding(Lifecycle::Active);
        let (cap, mode) = required();
        let mut authority = GrantTable::new().with_grant(
            Grant::new(original.subject, cap.clone(), mode).expiring_at(1),
        );
        let ticket = admit(
            &original,
            Trigger::Manual,
            &authority,
            &[(cap.clone(), mode)],
        )
        .unwrap();

        authority.set_now(1);
        assert_eq!(
            revalidate(&ticket, &original, &authority, &[(cap.clone(), mode)]),
            Err(AdmissionError::Unauthorized {
                cap: cap.clone(),
                mode
            })
        );
        let empty_authority = GrantTable::new();
        assert_eq!(
            revalidate(
                &ticket,
                &original,
                &empty_authority,
                &[(cap.clone(), mode)]
            ),
            Err(AdmissionError::Unauthorized {
                cap: cap.clone(),
                mode
            })
        );

        let mut revoked = original.clone();
        revoked.lifecycle = Lifecycle::Revoked;
        assert_eq!(
            revalidate(&ticket, &revoked, &authority, &[(cap.clone(), mode)]),
            Err(AdmissionError::Revoked)
        );

        let mut paused = original.clone();
        paused.lifecycle = Lifecycle::Paused;
        assert_eq!(
            revalidate(&ticket, &paused, &authority, &[(cap.clone(), mode)]),
            Err(AdmissionError::Paused)
        );

        for changed in [
            ResidentBinding {
                id: ResidentId([9; 16]),
                ..original.clone()
            },
            ResidentBinding {
                revision: BodyRevision([9; 32]),
                ..original.clone()
            },
            ResidentBinding {
                generation: 9,
                ..original.clone()
            },
            ResidentBinding {
                subject: Subject::new([9; 32]),
                ..original.clone()
            },
        ] {
            assert_eq!(
                revalidate(&ticket, &changed, &authority, &[(cap.clone(), mode)]),
                Err(AdmissionError::StaleBinding)
            );
        }
    }

    #[test]
    fn revalidation_checks_each_current_required_capability() {
        let active = binding(Lifecycle::Active);
        let read = (Cap::scope("resident/read").unwrap(), Mode::Read);
        let write = (Cap::scope("resident/write").unwrap(), Mode::Write);
        let read_authority = GrantTable::new().with_grant(Grant::new(
            active.subject,
            Cap::scope("resident/read").unwrap(),
            Mode::Read,
        ));
        let ticket = admit(&active, Trigger::Manual, &read_authority, &[read.clone()]).unwrap();
        assert_eq!(
            revalidate(&ticket, &active, &read_authority, &[write.clone()]),
            Err(AdmissionError::Unauthorized {
                cap: write.0.clone(),
                mode: write.1
            })
        );
        assert_eq!(
            revalidate(
                &ticket,
                &active,
                &read_authority,
                &[read.clone(), write.clone()],
            ),
            Err(AdmissionError::Unauthorized {
                cap: write.0.clone(),
                mode: write.1
            })
        );

        let write_authority = GrantTable::new().with_grant(Grant::new(
            active.subject,
            Cap::scope("resident/write").unwrap(),
            Mode::Write,
        ));
        let write_ticket = admit(&active, Trigger::Manual, &write_authority, &[write.clone()])
            .unwrap();
        assert_eq!(
            revalidate(&write_ticket, &active, &write_authority, &[read.clone()]),
            Err(AdmissionError::Unauthorized {
                cap: read.0.clone(),
                mode: read.1
            })
        );
    }

    #[test]
    fn revalidation_keeps_original_requirements_when_new_requirements_are_empty() {
        let active = binding(Lifecycle::Active);
        let read = (Cap::scope("resident/read").unwrap(), Mode::Read);
        let write = (Cap::scope("resident/write").unwrap(), Mode::Write);
        let authority = GrantTable::new()
            .with_grant(Grant::new(
                active.subject,
                Cap::scope("resident/read").unwrap(),
                Mode::Read,
            ))
            .with_grant(
                Grant::new(
                    active.subject,
                    Cap::scope("resident/write").unwrap(),
                    Mode::Write,
                )
                .expiring_at(1),
            );
        let ticket = admit(
            &active,
            Trigger::Manual,
            &authority,
            &[read.clone(), write.clone()],
        )
        .unwrap();
        let mut after_expiry = authority;
        after_expiry.set_now(1);
        assert_eq!(
            revalidate(&ticket, &active, &after_expiry, &[]),
            Err(AdmissionError::Unauthorized {
                cap: write.0,
                mode: write.1
            })
        );
    }

    #[test]
    fn trigger_validation_and_shapes_are_preserved() {
        let active = binding(Lifecycle::Active);
        let (cap, mode) = required();
        let authority = authority(active.subject, cap.clone(), mode);
        let required = [(cap, mode)];
        assert!(admit(&active, Trigger::Manual, &authority, &required).is_ok());
        assert!(admit(
            &active,
            Trigger::Clock { at_ms: 7 },
            &authority,
            &required
        )
        .is_ok());
        assert!(admit(
            &active,
            Trigger::Journal {
                source: " journal ".into(),
                first: 2,
                last: 2
            },
            &authority,
            &required
        )
        .is_ok());
        assert_eq!(
            admit(
                &active,
                Trigger::Journal {
                    source: "  ".into(),
                    first: 1,
                    last: 1
                },
                &authority,
                &required
            ),
            Err(AdmissionError::InvalidTrigger)
        );
        assert_eq!(
            admit(
                &active,
                Trigger::Journal {
                    source: "journal".into(),
                    first: 2,
                    last: 1
                },
                &authority,
                &required
            ),
            Err(AdmissionError::InvalidTrigger)
        );
    }

    #[test]
    fn revalidation_rejects_a_forged_invalid_trigger() {
        let active = binding(Lifecycle::Active);
        let (cap, mode) = required();
        let authority = authority(active.subject, cap.clone(), mode);
        let mut ticket = admit(&active, Trigger::Manual, &authority, &[(cap.clone(), mode)])
            .unwrap();
        ticket.trigger = Trigger::Journal {
            source: "".into(),
            first: 2,
            last: 1,
        };
        assert_eq!(
            revalidate(&ticket, &active, &authority, &[(cap, mode)]),
            Err(AdmissionError::InvalidTrigger)
        );
    }

    #[test]
    fn authority_is_subject_specific_even_for_equal_shaped_instances() {
        let active = binding(Lifecycle::Active);
        let (cap, mode) = required();
        let other = ResidentBinding {
            id: ResidentId([8; 16]),
            subject: active.subject,
            ..active.clone()
        };
        let authority = authority(active.subject, cap.clone(), mode);
        let ticket = admit(
            &active,
            Trigger::Manual,
            &authority,
            &[(cap.clone(), mode)]
        )
        .unwrap();
        assert_eq!(
            revalidate(&ticket, &other, &authority, &[(cap, mode)]),
            Err(AdmissionError::StaleBinding)
        );
    }
}
