// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! **hagiograph**, the history organ: which events were significant, and the
//! past they are judged against.
//!
//! A journal records everything that happened; this crate answers what a
//! journal cannot answer by itself. It keeps **standing marks** along axes a
//! product defines, with their holders, merging by maximum so forked worlds
//! join without a protocol. It judges **feats**: a feat beats a mark that
//! stood before the reckoning, and a first mark on an empty record is not
//! one. It gives a generated world a **past** by running the product's own
//! simulation through a seam for a set span before anyone steps in. Later it
//! holds what memory keeps: legends, memorials and epithets, whose presence
//! scales with retelling.
//!
//! The boundaries are the point:
//!
//! - **Not the storage of event history.** That is a `muniment::Journal` or a
//!   product's own log; this crate judges and generates history.
//! - **Not a simulation.** Deep time drives the product's simulation and never
//!   simulates anything itself.
//! - **Not an immutable exchange record.** That is an `eidetic::Codicil`.
//! - **Not descent.** That is `fili`, which records continuity of line across
//!   worlds.
//!
//! Rescoped 2026-09-16; see the hagiograph history organ plan under
//! `design_docs/eidetic_docs/implementation_strategy/`.

#![doc(html_no_source)]

mod deep_time;
mod reckoning;
mod record;

pub use deep_time::{DeepTime, DeepTimeError, Epochal, Handover, run};
pub use reckoning::{Entry, Judgement};
pub use record::{Mark, Record};
