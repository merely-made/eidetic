// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! One tokenizer, used for indexing and for querying.
//!
//! Genet supplies the versioned UAX #29 default word boundaries. Everything
//! after that seam is search policy: connector splitting, lowercasing and
//! optional stemming. Locale dictionaries remain a separate future profile.
//!
//! Public so `esp` can share it rather than grow a second token stream.

/// Boundary profile used before search-specific normalization.
pub const SEGMENTATION_PROFILE: &str = genet_text::PROFILE;

/// The tokenizer's name, recorded in the index spec.
pub const TOKENIZER_NAME: &str = "unicode-words";

/// A stemming hook. Left `None` by default — a title-and-URL corpus barely
/// needs stemming, and owning the tokenizer means owning that decision
/// rather than inheriting a stemmer crate.
pub type Stemmer = fn(&str) -> String;

/// Unicode word segmentation, lowercased, through an optional stemmer.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tokenizer {
    /// Applied to each lowercased token; identity when `None`.
    pub stem: Option<Stemmer>,
}

impl Tokenizer {
    /// The default tokenizer: segment, lowercase, no stemming.
    pub const fn new() -> Self {
        Self { stem: None }
    }

    /// The same tokenizer with a stemming hook installed.
    pub const fn with_stemmer(stem: Stemmer) -> Self {
        Self { stem: Some(stem) }
    }

    /// The shared boundary seam; search normalization follows separately.
    fn segment(text: &str) -> impl Iterator<Item = &str> {
        genet_text::words(text)
    }

    /// Tokens for one field of text, in order.
    pub fn tokens(&self, text: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.tokens_into(text, &mut out);
        out
    }

    /// The same, appending into a caller-owned buffer so a minting loop keeps
    /// one allocation instead of one per field per document.
    pub fn tokens_into(&self, text: &str, out: &mut Vec<String>) {
        for word in Self::segment(text) {
            // UAX #29 keeps `docs.example` and `3.14` whole (a full stop
            // between letters is MidNumLet). In a URL a dot is structure, not
            // orthography, so split the residual connectors back out.
            for piece in word.split(|c: char| !c.is_alphanumeric()) {
                if piece.is_empty() {
                    continue;
                }
                let lowered = if piece.is_ascii() {
                    piece.to_ascii_lowercase()
                } else {
                    piece.to_lowercase()
                };
                out.push(match self.stem {
                    Some(stem) => stem(&lowered),
                    None => lowered,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_titles_tokenize_into_lowercase_components() {
        let tokenizer = Tokenizer::new();
        assert_eq!(
            tokenizer.tokens("https://docs.example/rust/async/book/getting-started"),
            [
                "https", "docs", "example", "rust", "async", "book", "getting", "started"
            ]
        );
        assert_eq!(
            tokenizer.tokens("Vello Scene Encoding API"),
            ["vello", "scene", "encoding", "api"]
        );
        assert!(tokenizer.tokens("  ---  ").is_empty());
    }

    #[test]
    fn shared_boundaries_keep_the_existing_index_token_stream() {
        assert_eq!(
            Tokenizer::new().tokens("don't 3.14 foo_bar café"),
            ["don", "t", "3", "14", "foo", "bar", "café"]
        );
        assert_eq!(TOKENIZER_NAME, "unicode-words");
        assert_eq!(SEGMENTATION_PROFILE, "uax29-default-unicode-17.0.0");
    }

    #[test]
    fn the_stemmer_seam_is_a_no_op_until_installed() {
        assert_eq!(Tokenizer::new().tokens("Running"), ["running"]);
        fn chop(word: &str) -> String {
            word.trim_end_matches("ing").to_string()
        }
        assert_eq!(Tokenizer::with_stemmer(chop).tokens("Running"), ["runn"]);
    }

    #[test]
    fn segmentation_is_not_ascii_only() {
        let tokenizer = Tokenizer::new();
        assert_eq!(tokenizer.tokens("Grüße, Welt"), ["grüße", "welt"]);
        // UAX #29 alone has no dictionary, so unspaced scripts fall to one
        // token per ideograph. Recall still works, precision does not — the
        // default profile makes no dictionary-segmentation claim.
        assert_eq!(tokenizer.tokens("東京 tower"), ["東", "京", "tower"]);
    }
}
