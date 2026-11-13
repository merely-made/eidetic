// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! fieldbook — graph fields as portable data.
//!
//! Fields are the third graph primitive beside nodes and edges: [`field`],
//! [`field_ast`], [`coupling`] and [`edge_path`] define them as portable data
//! (identity, extent, the scalar/vector AST, how elements respond, how an edge
//! is drawn). Evaluation is not here; the numen crate in Seiche evaluates them.
//!
//! These modules were part of the `chartulary` crate until 2026-11-12, having
//! moved there from numen (Mere engine-split D49).

pub mod coupling;
pub mod edge_path;
pub mod field;
pub mod field_ast;
