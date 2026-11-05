// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Pure key-range arithmetic for backends whose native key order differs from
//! Rust's.
//!
//! Muniment's contract is Rust `str` order (code point order). IndexedDB
//! compares string keys by UTF-16 code units. The two orders disagree only
//! for characters at or above U+D800: a supplementary-plane character encodes
//! as a surrogate pair (D800-DFFF), which sorts below U+E000-U+FFFF in UTF-16
//! but above them by code point.
//!
//! A bound made only of characters below U+D800 compares identically against
//! every possible key in both orders: at the first differing position the
//! bound has a BMP non-surrogate unit, and a key's unit there is either a BMP
//! unit (same order) or a lead surrogate (greater in both). So a native range
//! built from such bounds selects a superset-or-equal of the Rust-order
//! result, and the caller's Rust post-filter makes it exact.
//!
//! This module has no browser dependency so it is unit-tested natively.

/// Whether `s` contains only characters below U+D800, so that native
/// (UTF-16) comparison against it agrees with Rust comparison.
pub fn is_order_safe(s: &str) -> bool {
    s.chars().all(|c| (c as u32) < 0xD800)
}

/// The smallest string greater than every string beginning with `prefix`,
/// or `None` when no such bound exists in the safe alphabet (empty prefix, or
/// a prefix consisting only of U+D7FF) or when `prefix` is not order-safe.
pub fn prefix_upper_bound(prefix: &str) -> Option<String> {
    if !is_order_safe(prefix) {
        return None;
    }
    let mut chars: Vec<char> = prefix.chars().collect();
    // U+D7FF has no safe successor (U+D800 is a surrogate, U+E000 is unsafe);
    // dropping it and carrying into the previous character still bounds the
    // prefix from above.
    while chars.last() == Some(&'\u{D7FF}') {
        chars.pop();
    }
    let last = chars.pop()?;
    let next = char::from_u32(last as u32 + 1)?;
    chars.push(next);
    Some(chars.into_iter().collect())
}

/// Optional native bounds: `lower` inclusive, `upper` exclusive.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeyBounds {
    pub lower: Option<String>,
    pub upper: Option<String>,
}

impl KeyBounds {
    /// True when neither side narrows the range (a full enumeration).
    pub fn is_unbounded(&self) -> bool {
        self.lower.is_none() && self.upper.is_none()
    }
}

/// Bounds for `list(prefix)`. Unbounded for an empty or unsafe prefix.
pub fn prefix_bounds(prefix: &str) -> KeyBounds {
    if prefix.is_empty() || !is_order_safe(prefix) {
        return KeyBounds::default();
    }
    KeyBounds {
        lower: Some(prefix.to_owned()),
        upper: prefix_upper_bound(prefix),
    }
}

/// Bounds for `scan(start, end)`, each side narrowed independently when safe.
/// Returns `None` when the Rust range `[start, end)` is empty.
pub fn scan_bounds(start: &str, end: &str) -> Option<KeyBounds> {
    if start >= end {
        return None;
    }
    Some(KeyBounds {
        lower: (!start.is_empty() && is_order_safe(start)).then(|| start.to_owned()),
        upper: is_order_safe(end).then(|| end.to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_chars() {
        assert!(is_order_safe(""));
        assert!(is_order_safe("abc\u{D7FF}"));
        assert!(!is_order_safe("\u{E000}"));
        assert!(!is_order_safe("a\u{10000}"));
    }

    #[test]
    fn upper_bound_increments_last_char() {
        assert_eq!(prefix_upper_bound("ab").as_deref(), Some("ac"));
        assert_eq!(prefix_upper_bound("journal/").as_deref(), Some("journal0"));
    }

    #[test]
    fn upper_bound_carries_past_d7ff() {
        assert_eq!(prefix_upper_bound("a\u{D7FF}").as_deref(), Some("b"));
        assert_eq!(
            prefix_upper_bound("a\u{D7FF}\u{D7FF}").as_deref(),
            Some("b")
        );
        assert_eq!(prefix_upper_bound("\u{D7FF}"), None);
    }

    #[test]
    fn upper_bound_none_for_empty_or_unsafe() {
        assert_eq!(prefix_upper_bound(""), None);
        assert_eq!(prefix_upper_bound("\u{E000}"), None);
        assert_eq!(prefix_upper_bound("a\u{10000}"), None);
    }

    #[test]
    fn upper_bound_exceeds_every_prefixed_key_including_supplementary() {
        let bound = prefix_upper_bound("ab").unwrap();
        for key in ["ab", "abz", "ab\u{E000}", "ab\u{10000}", "ab\u{10FFFF}"] {
            assert!(key < bound.as_str(), "{key:?}");
        }
        assert!("ac" >= bound.as_str());
    }

    #[test]
    fn prefix_bounds_cases() {
        assert!(prefix_bounds("").is_unbounded());
        assert!(prefix_bounds("\u{E000}").is_unbounded());
        assert_eq!(
            prefix_bounds("ab"),
            KeyBounds {
                lower: Some("ab".into()),
                upper: Some("ac".into())
            }
        );
        let open_top = prefix_bounds("\u{D7FF}");
        assert_eq!(open_top.lower.as_deref(), Some("\u{D7FF}"));
        assert_eq!(open_top.upper, None);
    }

    #[test]
    fn scan_bounds_cases() {
        assert_eq!(scan_bounds("b", "a"), None);
        assert_eq!(scan_bounds("a", "a"), None);
        assert_eq!(
            scan_bounds("a", "c"),
            Some(KeyBounds {
                lower: Some("a".into()),
                upper: Some("c".into())
            })
        );
        // Unsafe side falls back to open; safe side still narrows.
        let b = scan_bounds("a", "\u{E000}").unwrap();
        assert_eq!(b.lower.as_deref(), Some("a"));
        assert_eq!(b.upper, None);
        let b = scan_bounds("\u{E000}", "\u{10000}").unwrap();
        assert!(b.is_unbounded());
    }

    /// The orders disagree for these, so an unsafe bound must never be used.
    #[test]
    fn utf16_and_code_point_orders_disagree_above_d800() {
        let (bmp, supp) = ("\u{E000}", "\u{10000}");
        assert!(bmp < supp);
        assert!(bmp.encode_utf16().cmp(supp.encode_utf16()).is_gt());
    }

    /// For safe bounds, UTF-16 and Rust order agree against any key.
    #[test]
    fn safe_bounds_agree_in_both_orders() {
        let keys = [
            "",
            "a",
            "ab",
            "\u{D7FF}",
            "\u{E000}",
            "\u{FFFF}",
            "\u{10000}",
            "b\u{10FFFF}",
        ];
        for bound in ["", "a", "ab", "b", "\u{D7FF}", "a\u{D7FF}"] {
            for key in keys {
                let rust = key.cmp(bound);
                let utf16 = key.encode_utf16().cmp(bound.encode_utf16());
                assert_eq!(rust, utf16, "{key:?} vs {bound:?}");
            }
        }
    }
}
