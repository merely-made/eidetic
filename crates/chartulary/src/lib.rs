// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Transitional shim: chartulary's old paths over the crates it split into
//! (Eidetic `design_docs/eidetic_docs/implementation_strategy/2026-11-12_portolan_plan.md`).
//!
//! `chartulary::X` is `portolan::X`, `chartulary::stemma` is `hodograph`,
//! `chartulary::quads` is `quadstore`, and `chartulary::{field, field_ast,
//! coupling, edge_path}` are fieldbook's modules. The types are the same types.
//! Git-only and never published; it goes when no consumer names it (P4).

pub use portolan::*;

pub use fieldbook::{coupling, edge_path, field, field_ast};
pub use hodograph as stemma;
pub use quadstore as quads;
