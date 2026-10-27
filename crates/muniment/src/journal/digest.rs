// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A digest of a journal's leading entries, so a checkpoint can name the
//! history it summarizes.
//!
//! A checkpoint (a snapshot of the state the first `n` entries describe) is a
//! cache of a replay. It is only a faithful one if the journal it is read back
//! beside still begins with those `n` entries, and a length alone cannot say so.
//! [`Journal::prefix_digest`] gives the checkpoint's writer something to store
//! and its reader something to check.
//!
//! The digest is taken over each entry's canonical encoding, not over the bytes
//! a codec happened to store: those change with the codec and, for any entry
//! holding a hash-ordered map, from one save to the next. The canonical
//! encoding is the entry's serde stream with maps ordered by their encoded
//! keys, so it is the same for equal values in every process and every codec.
//! A set that serializes as a sequence is not reordered; an entry type that
//! holds one must serialize it in a stable order itself.

use std::fmt;

use serde::ser::{self, Serialize};

use crate::blob::Hash;
use crate::error::StoreError;

use super::log::Journal;

/// Domain separation, and the version of this definition: a change to the
/// encoding below changes this string.
const DOMAIN: &str = "muniment journal prefix digest v1";

impl<T: Serialize> Journal<T> {
    /// A digest of the first `len` entries, or `None` if the log holds fewer.
    ///
    /// It covers each entry whole (an entry that records its author or
    /// provenance binds them) and the count, and nothing else: not the log's
    /// [`id`](Journal::id), its [`provenance`](Journal::provenance) or the causes
    /// recorded beside the entries. So a prefix keeps its digest as the log
    /// grows, across saves and loads, and in a [`fork`](Journal::fork) that
    /// copied it. An empty prefix has a digest too.
    ///
    /// The cost is linear in `len`: every entry in the prefix is encoded and
    /// hashed. That is far less than replaying them.
    ///
    /// # Errors
    ///
    /// [`StoreError::Codec`] if an entry's `Serialize` implementation fails.
    pub fn prefix_digest(&self, len: usize) -> Result<Option<Hash>, StoreError> {
        let Some(prefix) = self.entries().get(..len) else {
            return Ok(None);
        };
        let mut hasher = blake3::Hasher::new_derive_key(DOMAIN);
        hasher.update(&(len as u64).to_le_bytes());
        let mut encoded = Vec::new();
        for entry in prefix {
            encoded.clear();
            entry
                .serialize(Canonical { out: &mut encoded })
                .map_err(|error| StoreError::Codec(error.0))?;
            hasher.update(&(encoded.len() as u64).to_le_bytes());
            hasher.update(&encoded);
        }
        Ok(Some(Hash::from_bytes(*hasher.finalize().as_bytes())))
    }
}

mod tag {
    pub const UNIT: u8 = 0;
    pub const BOOL: u8 = 1;
    pub const UINT: u8 = 2;
    pub const INT: u8 = 3;
    pub const FLOAT: u8 = 4;
    pub const CHAR: u8 = 5;
    pub const STR: u8 = 6;
    pub const BYTES: u8 = 7;
    pub const NONE: u8 = 8;
    pub const SOME: u8 = 9;
    pub const SEQ: u8 = 10;
    pub const MAP: u8 = 11;
    pub const STRUCT: u8 = 12;
    pub const VARIANT: u8 = 13;
    pub const END: u8 = 14;
}

#[derive(Debug)]
struct CanonicalError(String);

impl fmt::Display for CanonicalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CanonicalError {}

impl ser::Error for CanonicalError {
    fn custom<M: fmt::Display>(message: M) -> Self {
        Self(message.to_string())
    }
}

/// Writes a value's canonical encoding: tagged, length-delimited where a length
/// is known and terminated where it is not, with map entries sorted.
struct Canonical<'a> {
    out: &'a mut Vec<u8>,
}

impl Canonical<'_> {
    fn text(&mut self, text: &str) {
        self.out.push(tag::STR);
        self.out.extend((text.len() as u64).to_le_bytes());
        self.out.extend(text.as_bytes());
    }

    fn variant(&mut self, name: &str) {
        self.out.push(tag::VARIANT);
        self.text(name);
    }
}

macro_rules! unsigned {
    ($($method:ident: $type:ty),*) => {$(
        fn $method(self, v: $type) -> Result<(), CanonicalError> {
            self.out.push(tag::UINT);
            self.out.extend((v as u128).to_le_bytes());
            Ok(())
        }
    )*};
}

macro_rules! signed {
    ($($method:ident: $type:ty),*) => {$(
        fn $method(self, v: $type) -> Result<(), CanonicalError> {
            self.out.push(tag::INT);
            self.out.extend((v as i128).to_le_bytes());
            Ok(())
        }
    )*};
}

impl<'a> ser::Serializer for Canonical<'a> {
    type Ok = ();
    type Error = CanonicalError;
    type SerializeSeq = Compound<'a>;
    type SerializeTuple = Compound<'a>;
    type SerializeTupleStruct = Compound<'a>;
    type SerializeTupleVariant = Compound<'a>;
    type SerializeMap = Pairs<'a>;
    type SerializeStruct = Compound<'a>;
    type SerializeStructVariant = Compound<'a>;

    unsigned!(serialize_u8: u8, serialize_u16: u16, serialize_u32: u32, serialize_u64: u64, serialize_u128: u128);
    signed!(serialize_i8: i8, serialize_i16: i16, serialize_i32: i32, serialize_i64: i64, serialize_i128: i128);

    fn is_human_readable(&self) -> bool {
        false
    }

    fn serialize_bool(self, v: bool) -> Result<(), CanonicalError> {
        self.out.extend([tag::BOOL, v as u8]);
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<(), CanonicalError> {
        self.serialize_f64(f64::from(v))
    }

    fn serialize_f64(self, v: f64) -> Result<(), CanonicalError> {
        let v = if v.is_nan() { f64::NAN } else { v };
        self.out.push(tag::FLOAT);
        self.out.extend(v.to_bits().to_le_bytes());
        Ok(())
    }

    fn serialize_char(self, v: char) -> Result<(), CanonicalError> {
        self.out.push(tag::CHAR);
        self.out.extend((v as u32).to_le_bytes());
        Ok(())
    }

    fn serialize_str(mut self, v: &str) -> Result<(), CanonicalError> {
        self.text(v);
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<(), CanonicalError> {
        self.out.push(tag::BYTES);
        self.out.extend((v.len() as u64).to_le_bytes());
        self.out.extend(v);
        Ok(())
    }

    fn serialize_none(self) -> Result<(), CanonicalError> {
        self.out.push(tag::NONE);
        Ok(())
    }

    fn serialize_some<V: ?Sized + Serialize>(self, value: &V) -> Result<(), CanonicalError> {
        self.out.push(tag::SOME);
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<(), CanonicalError> {
        self.out.push(tag::UNIT);
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), CanonicalError> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        mut self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<(), CanonicalError> {
        self.variant(variant);
        self.out.push(tag::UNIT);
        Ok(())
    }

    fn serialize_newtype_struct<V: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &V,
    ) -> Result<(), CanonicalError> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<V: ?Sized + Serialize>(
        mut self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        value: &V,
    ) -> Result<(), CanonicalError> {
        self.variant(variant);
        value.serialize(self)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Compound<'a>, CanonicalError> {
        self.out.push(tag::SEQ);
        Ok(Compound { out: self.out })
    }

    fn serialize_tuple(self, len: usize) -> Result<Compound<'a>, CanonicalError> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Compound<'a>, CanonicalError> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        mut self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Compound<'a>, CanonicalError> {
        self.variant(variant);
        self.serialize_seq(Some(len))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Pairs<'a>, CanonicalError> {
        Ok(Pairs {
            out: self.out,
            pairs: Vec::new(),
            key: None,
        })
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Compound<'a>, CanonicalError> {
        self.out.push(tag::STRUCT);
        Ok(Compound { out: self.out })
    }

    fn serialize_struct_variant(
        mut self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Compound<'a>, CanonicalError> {
        self.variant(variant);
        self.serialize_struct(variant, len)
    }
}

/// A sequence, tuple or struct: its elements are written in the order given.
struct Compound<'a> {
    out: &'a mut Vec<u8>,
}

impl Compound<'_> {
    fn element<V: ?Sized + Serialize>(&mut self, value: &V) -> Result<(), CanonicalError> {
        value.serialize(Canonical { out: self.out })
    }

    fn finish(self) -> Result<(), CanonicalError> {
        self.out.push(tag::END);
        Ok(())
    }
}

impl ser::SerializeSeq for Compound<'_> {
    type Ok = ();
    type Error = CanonicalError;

    fn serialize_element<V: ?Sized + Serialize>(&mut self, value: &V) -> Result<(), CanonicalError> {
        self.element(value)
    }

    fn end(self) -> Result<(), CanonicalError> {
        self.finish()
    }
}

impl ser::SerializeTuple for Compound<'_> {
    type Ok = ();
    type Error = CanonicalError;

    fn serialize_element<V: ?Sized + Serialize>(&mut self, value: &V) -> Result<(), CanonicalError> {
        self.element(value)
    }

    fn end(self) -> Result<(), CanonicalError> {
        self.finish()
    }
}

impl ser::SerializeTupleStruct for Compound<'_> {
    type Ok = ();
    type Error = CanonicalError;

    fn serialize_field<V: ?Sized + Serialize>(&mut self, value: &V) -> Result<(), CanonicalError> {
        self.element(value)
    }

    fn end(self) -> Result<(), CanonicalError> {
        self.finish()
    }
}

impl ser::SerializeTupleVariant for Compound<'_> {
    type Ok = ();
    type Error = CanonicalError;

    fn serialize_field<V: ?Sized + Serialize>(&mut self, value: &V) -> Result<(), CanonicalError> {
        self.element(value)
    }

    fn end(self) -> Result<(), CanonicalError> {
        self.finish()
    }
}

impl ser::SerializeStruct for Compound<'_> {
    type Ok = ();
    type Error = CanonicalError;

    fn serialize_field<V: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &V,
    ) -> Result<(), CanonicalError> {
        Canonical { out: self.out }.text(key);
        self.element(value)
    }

    fn end(self) -> Result<(), CanonicalError> {
        self.finish()
    }
}

impl ser::SerializeStructVariant for Compound<'_> {
    type Ok = ();
    type Error = CanonicalError;

    fn serialize_field<V: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &V,
    ) -> Result<(), CanonicalError> {
        ser::SerializeStruct::serialize_field(self, key, value)
    }

    fn end(self) -> Result<(), CanonicalError> {
        self.finish()
    }
}

/// A map: each key and value is encoded on its own, and the pairs are written
/// in the order of their encoded keys, so a hash-ordered map encodes the same
/// whatever order it iterates in.
struct Pairs<'a> {
    out: &'a mut Vec<u8>,
    pairs: Vec<(Vec<u8>, Vec<u8>)>,
    key: Option<Vec<u8>>,
}

fn encoded<V: ?Sized + Serialize>(value: &V) -> Result<Vec<u8>, CanonicalError> {
    let mut bytes = Vec::new();
    value.serialize(Canonical { out: &mut bytes })?;
    Ok(bytes)
}

impl ser::SerializeMap for Pairs<'_> {
    type Ok = ();
    type Error = CanonicalError;

    fn serialize_key<K: ?Sized + Serialize>(&mut self, key: &K) -> Result<(), CanonicalError> {
        self.key = Some(encoded(key)?);
        Ok(())
    }

    fn serialize_value<V: ?Sized + Serialize>(&mut self, value: &V) -> Result<(), CanonicalError> {
        let key = self
            .key
            .take()
            .ok_or_else(|| CanonicalError("map value without a key".into()))?;
        self.pairs.push((key, encoded(value)?));
        Ok(())
    }

    fn end(mut self) -> Result<(), CanonicalError> {
        self.pairs.sort();
        self.out.push(tag::MAP);
        for (key, value) in self.pairs {
            self.out.extend(key);
            self.out.extend(value);
        }
        self.out.push(tag::END);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::{JsonSlots, LogId, MemoryBackend};

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct Edit {
        author: String,
        at_ms: u64,
        fields: HashMap<String, String>,
    }

    fn edit(author: &str, at_ms: u64, fields: usize) -> Edit {
        Edit {
            author: author.into(),
            at_ms,
            fields: (0..fields)
                .map(|n| (format!("key-{n:03}"), format!("value-{n:03}")))
                .collect(),
        }
    }

    fn journal(edits: impl IntoIterator<Item = Edit>) -> Journal<Edit> {
        let mut journal = Journal::new();
        for edit in edits {
            journal.append(edit);
        }
        journal
    }

    fn digest(journal: &Journal<Edit>, len: usize) -> Hash {
        journal.prefix_digest(len).unwrap().expect("within the log")
    }

    #[test]
    fn a_prefix_longer_than_the_log_has_no_digest_and_an_empty_one_has() {
        let log = journal([edit("a", 1, 2)]);
        assert_eq!(log.prefix_digest(2).unwrap(), None);
        assert!(log.prefix_digest(1).unwrap().is_some());
        assert!(journal([]).prefix_digest(0).unwrap().is_some());
    }

    #[test]
    fn a_prefix_keeps_its_digest_as_the_log_grows() {
        let mut log = journal([edit("a", 1, 3), edit("b", 2, 3)]);
        let before = digest(&log, 2);
        log.append(edit("c", 3, 3));
        assert_eq!(digest(&log, 2), before);
        assert_ne!(digest(&log, 3), before);
    }

    #[test]
    fn the_digest_follows_each_entry_whole_and_their_order() {
        let base = journal([edit("a", 1, 3), edit("b", 2, 3)]);
        let reference = digest(&base, 2);
        assert_ne!(digest(&journal([edit("b", 2, 3), edit("a", 1, 3)]), 2), reference);
        assert_ne!(digest(&journal([edit("a", 1, 3), edit("c", 2, 3)]), 2), reference, "author");
        assert_ne!(digest(&journal([edit("a", 1, 3), edit("b", 9, 3)]), 2), reference, "time");
        assert_ne!(digest(&journal([edit("a", 1, 3), edit("b", 2, 4)]), 2), reference, "payload");
        assert_ne!(digest(&base, 1), reference, "length");
    }

    #[test]
    fn equal_entries_digest_alike_whatever_order_their_maps_iterate_in() {
        // Each journal builds its own hash tables, with their own random seeds.
        let digests: Vec<Hash> = (0..8)
            .map(|_| digest(&journal([edit("a", 1, 64), edit("b", 2, 64)]), 2))
            .collect();
        assert!(digests.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn the_digest_survives_a_save_and_a_load() {
        pollster::block_on(async {
            let slots = JsonSlots::new(MemoryBackend::new());
            let log = journal([edit("a", 1, 64), edit("b", 2, 64)]);
            let before = digest(&log, 2);
            log.save(&slots, "log").await.unwrap();
            for _ in 0..4 {
                let loaded: Journal<Edit> = Journal::load(&slots, "log").await.unwrap();
                assert_eq!(digest(&loaded, 2), before);
                loaded.save(&slots, "log").await.unwrap();
            }
        });
    }

    #[test]
    fn the_digest_binds_entries_only() {
        let anonymous = journal([edit("a", 1, 2), edit("b", 2, 2)]);
        let mut named = Journal::with_id(LogId::new("named"));
        named.append(edit("a", 1, 2));
        named.append(edit("b", 2, 2));
        assert_eq!(digest(&named, 2), digest(&anonymous, 2), "identity is not an entry");

        let fork = anonymous.fork(LogId::new("fork"));
        assert_eq!(digest(&fork, 2), digest(&anonymous, 2), "a fork shares its prefix");
    }

    #[test]
    fn enums_options_and_nested_collections_encode_distinctly() {
        #[derive(Serialize)]
        enum Entry {
            Unit,
            Newtype(Option<u32>),
            Tuple(u8, Vec<String>),
            Struct { value: i64 },
        }
        let digest = |entry: Entry| {
            let mut log = Journal::new();
            log.append(entry);
            log.prefix_digest(1).unwrap().unwrap()
        };
        let all = [
            digest(Entry::Unit),
            digest(Entry::Newtype(None)),
            digest(Entry::Newtype(Some(0))),
            digest(Entry::Tuple(0, vec![])),
            digest(Entry::Tuple(0, vec![String::new()])),
            digest(Entry::Struct { value: 0 }),
            digest(Entry::Struct { value: -1 }),
        ];
        for (n, a) in all.iter().enumerate() {
            for b in &all[n + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
