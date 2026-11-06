// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Conversion between Unicode-scalar passage selectors and UTF-8 byte ranges.
//!
//! Eidetic selectors count `char`s; Alembic workshop selectors are UTF-8 byte
//! offsets. Conversion must run against the exact retained text.
use super::recall::PassageSelector;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Half-open UTF-8 byte offsets into the exact retained text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectorError {
    /// `start > end`.
    Reversed,
    /// An offset exceeds the text length (`len` is in the unit being converted from).
    OutOfBounds { len: usize },
    /// A byte offset splits a UTF-8 sequence.
    NotCharBoundary { offset: usize },
}

impl fmt::Display for SelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reversed => write!(f, "selector start is after end"),
            Self::OutOfBounds { len } => write!(f, "selector offset exceeds text length {len}"),
            Self::NotCharBoundary { offset } => {
                write!(f, "byte offset {offset} is not a UTF-8 character boundary")
            }
        }
    }
}
impl std::error::Error for SelectorError {}

impl PassageSelector {
    /// Convert char offsets to byte offsets in `text`, in one pass.
    ///
    /// # Errors
    /// `Reversed`, or `OutOfBounds` when an offset exceeds the char length.
    pub fn to_byte_range(&self, text: &str) -> Result<ByteRange, SelectorError> {
        if self.start > self.end {
            return Err(SelectorError::Reversed);
        }
        let (mut start, mut end) = (None, None);
        let mut count = 0usize;
        for (byte, _) in text.char_indices() {
            if count == self.start {
                start = Some(byte);
            }
            if count == self.end {
                end = Some(byte);
                break;
            }
            count += 1;
        }
        // Reaching the end of the text puts `count` at the char length.
        let at_end = |c: usize| (c == count).then_some(text.len());
        let start = start.or_else(|| at_end(self.start));
        let end = end.or_else(|| at_end(self.end));
        match (start, end) {
            (Some(start), Some(end)) => Ok(ByteRange { start, end }),
            _ => Err(SelectorError::OutOfBounds { len: count }),
        }
    }

    /// Convert byte offsets to char offsets in `text`, in one pass.
    ///
    /// # Errors
    /// `Reversed`, `OutOfBounds`, or `NotCharBoundary` for an offset inside a UTF-8 sequence.
    pub fn from_byte_range(range: ByteRange, text: &str) -> Result<Self, SelectorError> {
        if range.start > range.end {
            return Err(SelectorError::Reversed);
        }
        if range.end > text.len() {
            return Err(SelectorError::OutOfBounds { len: text.len() });
        }
        for offset in [range.start, range.end] {
            if !text.is_char_boundary(offset) {
                return Err(SelectorError::NotCharBoundary { offset });
            }
        }
        let mut start = 0usize;
        let mut count = 0usize;
        for (byte, _) in text.char_indices() {
            if byte == range.start {
                start = count;
            }
            if byte >= range.end {
                break;
            }
            count += 1;
        }
        let end = if range.end == text.len() {
            text.chars().count()
        } else {
            count
        };
        let start = if range.start == text.len() {
            end
        } else {
            start
        };
        Ok(Self { start, end })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(start: usize, end: usize) -> PassageSelector {
        PassageSelector { start, end }
    }
    fn br(start: usize, end: usize) -> ByteRange {
        ByteRange { start, end }
    }

    #[test]
    fn ascii_is_identity() {
        assert_eq!(sel(1, 3).to_byte_range("abcd"), Ok(br(1, 3)));
        assert_eq!(
            PassageSelector::from_byte_range(br(1, 3), "abcd"),
            Ok(sel(1, 3))
        );
    }

    #[test]
    fn multibyte_and_supplementary() {
        let t = "aé中😀z"; // 1 + 2 + 3 + 4 + 1 bytes
        assert_eq!(sel(1, 2).to_byte_range(t), Ok(br(1, 3)));
        assert_eq!(sel(2, 3).to_byte_range(t), Ok(br(3, 6)));
        assert_eq!(sel(3, 4).to_byte_range(t), Ok(br(6, 10)));
        assert_eq!(sel(0, 5).to_byte_range(t), Ok(br(0, 11)));
        assert_eq!(
            PassageSelector::from_byte_range(br(6, 10), t),
            Ok(sel(3, 4))
        );
    }

    #[test]
    fn round_trip_every_boundary() {
        let t = "aé中😀z";
        let n = t.chars().count();
        for s in 0..=n {
            for e in s..=n {
                let r = sel(s, e).to_byte_range(t).unwrap();
                assert_eq!(
                    &t[r.start..r.end],
                    t.chars().skip(s).take(e - s).collect::<String>()
                );
                assert_eq!(PassageSelector::from_byte_range(r, t), Ok(sel(s, e)));
            }
        }
    }

    #[test]
    fn mid_sequence_refused() {
        let t = "aé中😀z";
        assert_eq!(
            PassageSelector::from_byte_range(br(0, 2), t),
            Err(SelectorError::NotCharBoundary { offset: 2 })
        );
        assert_eq!(
            PassageSelector::from_byte_range(br(4, 6), t),
            Err(SelectorError::NotCharBoundary { offset: 4 })
        );
        assert_eq!(
            PassageSelector::from_byte_range(br(7, 10), t),
            Err(SelectorError::NotCharBoundary { offset: 7 })
        );
    }

    #[test]
    fn out_of_bounds_both_units() {
        let t = "aé"; // 2 chars, 3 bytes
        assert_eq!(
            sel(0, 3).to_byte_range(t),
            Err(SelectorError::OutOfBounds { len: 2 })
        );
        assert_eq!(
            sel(3, 3).to_byte_range(t),
            Err(SelectorError::OutOfBounds { len: 2 })
        );
        assert_eq!(
            PassageSelector::from_byte_range(br(0, 4), t),
            Err(SelectorError::OutOfBounds { len: 3 })
        );
        assert_eq!(
            PassageSelector::from_byte_range(br(4, 4), t),
            Err(SelectorError::OutOfBounds { len: 3 })
        );
    }

    #[test]
    fn reversed() {
        assert_eq!(sel(2, 1).to_byte_range("abc"), Err(SelectorError::Reversed));
        assert_eq!(
            PassageSelector::from_byte_range(br(2, 1), "abc"),
            Err(SelectorError::Reversed)
        );
    }

    #[test]
    fn empty_and_end_of_text() {
        let t = "é中";
        assert_eq!(sel(1, 1).to_byte_range(t), Ok(br(2, 2)));
        assert_eq!(sel(2, 2).to_byte_range(t), Ok(br(5, 5)));
        assert_eq!(sel(1, 2).to_byte_range(t), Ok(br(2, 5)));
        assert_eq!(PassageSelector::from_byte_range(br(5, 5), t), Ok(sel(2, 2)));
        assert_eq!(PassageSelector::from_byte_range(br(0, 0), t), Ok(sel(0, 0)));
        assert_eq!(sel(0, 0).to_byte_range(""), Ok(br(0, 0)));
        assert_eq!(
            PassageSelector::from_byte_range(br(0, 0), ""),
            Ok(sel(0, 0))
        );
    }
}
