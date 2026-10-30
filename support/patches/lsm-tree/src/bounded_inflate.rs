// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Incremental Miniz output under independent work and allocation ledgers.

use crate::bounded_budget::{Budget, BudgetError, BudgetKind};
use miniz_oxide::inflate::{
    TINFLStatus,
    core::{
        DecompressorOxide, decompress, inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
    },
};

pub(crate) enum InflateError {
    Budget(BudgetError),
    DecodedLimit,
    Corrupt,
    Allocation,
}

pub(crate) fn inflate(
    encoded: &[u8],
    limit: usize,
    allocation_limit: usize,
    metadata: bool,
    call: &mut Budget,
    operation: &mut Budget,
) -> Result<(Vec<u8>, usize), InflateError> {
    let mut output = Vec::new();
    let mut decoder = DecompressorOxide::new();
    let mut consumed = 0usize;
    let mut written = 0usize;
    let mut requested = 0usize;
    loop {
        if written >= limit {
            return Err(InflateError::DecodedLimit);
        }
        let available = call
            .remaining(BudgetKind::DecodeBytes)
            .min(operation.remaining(BudgetKind::DecodeBytes));
        if available == 0 {
            let error = Budget::charge_both(call, operation, BudgetKind::DecodeBytes, 1)
                .expect_err("zero remaining budget");
            return Err(InflateError::Budget(error));
        }
        // No step can produce more than the prechecked remaining work; actual
        // output is debited immediately after it is produced, before another step.
        let step = usize::try_from(available)
            .unwrap_or(usize::MAX)
            .min(4096)
            .min(limit - written);
        let target = written.checked_add(step).ok_or(InflateError::Allocation)?;
        if target > output.capacity() {
            requested = requested
                .checked_add(target)
                .ok_or(InflateError::Allocation)?;
            if requested > allocation_limit {
                return Err(InflateError::Allocation);
            }
            if metadata {
                Budget::charge_both(call, operation, BudgetKind::MetadataBytes, target as u64)
                    .map_err(InflateError::Budget)?;
            }
            Budget::charge_both(call, operation, BudgetKind::AllocationBytes, target as u64)
                .map_err(InflateError::Budget)?;
            output
                .try_reserve_exact(target - output.len())
                .map_err(|_| InflateError::Allocation)?;
        }
        output.resize(target, 0);
        let (status, input, produced) = decompress(
            &mut decoder,
            &encoded[consumed..],
            &mut output,
            written,
            TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
        );
        Budget::charge_both(call, operation, BudgetKind::DecodeBytes, produced as u64)
            .map_err(InflateError::Budget)?;
        consumed = consumed.checked_add(input).ok_or(InflateError::Corrupt)?;
        written = written.checked_add(produced).ok_or(InflateError::Corrupt)?;
        match status {
            TINFLStatus::Done => {
                output.truncate(written);
                return Ok((output, requested));
            }
            TINFLStatus::HasMoreOutput => {}
            _ => return Err(InflateError::Corrupt),
        }
    }
}
