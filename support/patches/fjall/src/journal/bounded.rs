// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Non-mutating v2 journal decoding. Whole-store admission is a separate step.

use crate::bounded_recovery::{
    JournalPlan, RecoveryBudget, RecoveryError, RecoveryLimitKind as Kind,
};
use std::{
    fs::File,
    hash::Hasher,
    io::{self, Read, Seek},
    path::Path,
};

pub(crate) struct PreparedItem {
    pub(crate) partition: String,
    pub(crate) key: Vec<u8>,
    pub(crate) value: Vec<u8>,
    pub(crate) value_type: lsm_tree::ValueType,
}

pub(crate) struct PreparedBatch {
    pub(crate) seqno: u64,
    pub(crate) items: Vec<PreparedItem>,
}

enum ReadError {
    Tail,
    Padding,
    Failure(RecoveryError),
}
impl From<RecoveryError> for ReadError {
    fn from(value: RecoveryError) -> Self {
        Self::Failure(value)
    }
}

struct Reader<'a> {
    file: File,
    budget: &'a mut RecoveryBudget,
}
impl Reader<'_> {
    fn bytes(&mut self, bytes: &mut [u8]) -> Result<(), ReadError> {
        self.budget.charge(Kind::InputBytes, bytes.len() as u64)?;
        match self.file.read_exact(bytes) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => Err(ReadError::Tail),
            Err(err) => Err(ReadError::Failure(RecoveryError::Io(err))),
        }
    }
    fn byte(&mut self) -> Result<u8, ReadError> {
        let mut bytes = [0; 1];
        self.bytes(&mut bytes)?;
        Ok(bytes[0])
    }
    fn u16(&mut self) -> Result<u16, ReadError> {
        let mut bytes = [0; 2];
        self.bytes(&mut bytes)?;
        Ok(u16::from_be_bytes(bytes))
    }
    fn u32(&mut self) -> Result<u32, ReadError> {
        let mut bytes = [0; 4];
        self.bytes(&mut bytes)?;
        Ok(u32::from_be_bytes(bytes))
    }
    fn u64(&mut self) -> Result<u64, ReadError> {
        let mut bytes = [0; 8];
        self.bytes(&mut bytes)?;
        Ok(u64::from_be_bytes(bytes))
    }
    fn buffer(&mut self, length: usize) -> Result<Vec<u8>, ReadError> {
        let end = self
            .file
            .metadata()
            .map_err(|error| ReadError::Failure(RecoveryError::Io(error)))?
            .len();
        let position = self
            .file
            .stream_position()
            .map_err(|error| ReadError::Failure(RecoveryError::Io(error)))?;
        let remaining = end.saturating_sub(position);
        if (length as u64) > remaining {
            return if remaining == 0 {
                Err(ReadError::Tail)
            } else {
                Err(RecoveryError::Corruption("ambiguous truncated journal payload".into()).into())
            };
        }
        // Reserve input before requesting a buffer. A value that cannot be read
        // under the current input budget must not be allocated first.
        self.budget.charge(Kind::InputBytes, length as u64)?;
        self.budget.charge(Kind::AllocationBytes, length as u64)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(length).map_err(|_| {
            ReadError::Failure(RecoveryError::Io(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "journal allocation failed",
            )))
        })?;
        bytes.resize(length, 0);
        match self.file.read_exact(&mut bytes) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Err(ReadError::Tail),
            Err(err) => return Err(ReadError::Failure(RecoveryError::Io(err))),
        }
        Ok(bytes)
    }
    fn item(&mut self, hasher: &mut xxhash_rust::xxh3::Xxh3) -> Result<PreparedItem, ReadError> {
        hasher.update(&[2]);
        let tag = self.byte()?;
        hasher.update(&[tag]);
        let value_type = lsm_tree::ValueType::try_from(tag)
            .map_err(|_| RecoveryError::Corruption("invalid journal value type".into()))?;
        let partition_len = usize::from(self.byte()?);
        hasher.update(&[partition_len as u8]);
        if partition_len > self.budget.limits.max_key_bytes {
            return Err(RecoveryError::Limit {
                kind: Kind::KeyBytes,
                limit: self.budget.limits.max_key_bytes as u64,
            }
            .into());
        }
        self.budget
            .charge(Kind::ReplayBytes, partition_len as u64)?;
        let partition = self.buffer(partition_len)?;
        hasher.update(&partition);
        let partition = String::from_utf8(partition)
            .map_err(|_| RecoveryError::Corruption("invalid journal partition name".into()))?;
        let key_len = usize::from(self.u16()?);
        hasher.update(&(key_len as u16).to_be_bytes());
        let combined = partition_len
            .checked_add(key_len)
            .ok_or(RecoveryError::Limit {
                kind: Kind::KeyBytes,
                limit: self.budget.limits.max_key_bytes as u64,
            })?;
        if combined > self.budget.limits.max_key_bytes {
            return Err(RecoveryError::Limit {
                kind: Kind::KeyBytes,
                limit: self.budget.limits.max_key_bytes as u64,
            }
            .into());
        }
        self.budget.charge(Kind::ReplayBytes, key_len as u64)?;
        let key = self.buffer(key_len)?;
        hasher.update(&key);
        let value_len = self.u32()? as usize;
        hasher.update(&(value_len as u32).to_be_bytes());
        if value_len > self.budget.limits.max_value_bytes {
            return Err(RecoveryError::Limit {
                kind: Kind::ValueBytes,
                limit: self.budget.limits.max_value_bytes as u64,
            }
            .into());
        }
        self.budget.charge(Kind::ReplayBytes, value_len as u64)?;
        let value = self.buffer(value_len)?;
        hasher.update(&value);
        Ok(PreparedItem {
            partition,
            key,
            value,
            value_type,
        })
    }
}

pub(crate) fn prepare(
    path: &Path,
    budget: &mut RecoveryBudget,
) -> Result<JournalPlan, RecoveryError> {
    super::bounded_set::charge_path_io(path, budget)?;
    let mut reader = Reader {
        file: File::open(path).map_err(RecoveryError::Io)?,
        budget,
    };
    let mut plan = JournalPlan {
        batches: Vec::new(),
        truncate_to: None,
    };
    let mut valid_pos = 0;
    loop {
        let position = reader.file.stream_position().map_err(RecoveryError::Io)?;
        let length = reader.file.metadata().map_err(RecoveryError::Io)?.len();
        if position == length {
            return Ok(plan);
        }
        let next = read_batch(&mut reader);
        match next {
            Ok(batch) => {
                super::bounded_set::reserve_one(&mut plan.batches, reader.budget)?;
                plan.batches.push(batch);
                valid_pos = reader.file.stream_position().map_err(RecoveryError::Io)?;
            }
            Err(ReadError::Tail) => {
                let file_len = reader.file.metadata().map_err(RecoveryError::Io)?.len();
                if valid_pos < file_len {
                    plan.truncate_to = Some(valid_pos);
                }
                return Ok(plan);
            }
            Err(ReadError::Padding) => {
                verify_zero_padding(&mut reader)?;
                plan.truncate_to = Some(valid_pos);
                return Ok(plan);
            }
            Err(ReadError::Failure(err)) => return Err(err),
        }
    }
}

fn read_batch(reader: &mut Reader<'_>) -> Result<PreparedBatch, ReadError> {
    match reader.byte()? {
        1 => {}
        0 => return Err(ReadError::Padding),
        _ => return Err(RecoveryError::Corruption("invalid journal start marker".into()).into()),
    }
    reader.budget.charge(Kind::ReplayItems, 1)?;
    let count = reader.u32()?;
    let seqno = reader.u64()?;
    let compression = [reader.byte()?, reader.byte()?];
    if compression != [0, 0] {
        return match compression {
            [1, 0] | [2, 0..=10] => Err(RecoveryError::Unsupported("compressed journal").into()),
            _ => Err(RecoveryError::Corruption("invalid journal compression".into()).into()),
        };
    }
    let remaining = reader
        .budget
        .limits
        .max_replay_items
        .saturating_sub(reader.budget.replay_items);
    if u64::from(count)
        .checked_add(1)
        .is_none_or(|x| x > remaining)
    {
        return Err(RecoveryError::Limit {
            kind: Kind::ReplayItems,
            limit: reader.budget.limits.max_replay_items,
        }
        .into());
    }
    let available = reader
        .file
        .metadata()
        .map_err(|e| ReadError::Failure(RecoveryError::Io(e)))?
        .len()
        .saturating_sub(
            reader
                .file
                .stream_position()
                .map_err(|e| ReadError::Failure(RecoveryError::Io(e)))?,
        );
    if u64::from(count).saturating_mul(9).saturating_add(13) > available {
        return if available == 0 {
            Err(ReadError::Tail)
        } else {
            Err(RecoveryError::Corruption("ambiguous incomplete journal batch".into()).into())
        };
    }
    let capacity = usize::try_from(count).map_err(|_| RecoveryError::Limit {
        kind: Kind::MetadataBytes,
        limit: reader.budget.limits.max_metadata_bytes,
    })?;
    let bytes = capacity
        .checked_mul(std::mem::size_of::<PreparedItem>())
        .ok_or(RecoveryError::Limit {
            kind: Kind::MetadataBytes,
            limit: reader.budget.limits.max_metadata_bytes,
        })?;
    reader.budget.metadata(bytes as u64)?;
    let mut items = Vec::new();
    items.try_reserve_exact(capacity).map_err(|_| {
        ReadError::Failure(RecoveryError::Io(io::Error::new(
            io::ErrorKind::OutOfMemory,
            "journal batch allocation failed",
        )))
    })?;
    reader.budget.charge(
        Kind::AllocationBytes,
        std::mem::size_of::<xxhash_rust::xxh3::Xxh3>() as u64,
    )?;
    let mut hasher = xxhash_rust::xxh3::Xxh3::new();
    for _ in 0..count {
        reader.budget.charge(Kind::ReplayItems, 1)?;
        match reader.byte()? {
            2 => {}
            3 => {
                return Err(
                    RecoveryError::Corruption("journal batch has too few items".into()).into(),
                )
            }
            0 => return Err(ReadError::Padding),
            _ => {
                return Err(RecoveryError::Corruption("invalid journal item marker".into()).into())
            }
        }
        items.push(reader.item(&mut hasher)?);
    }
    reader.budget.charge(Kind::ReplayItems, 1)?;
    match reader.byte()? {
        3 => {}
        2 => {
            return Err(RecoveryError::Corruption("journal batch has too many items".into()).into())
        }
        0 => return Err(ReadError::Padding),
        _ => return Err(RecoveryError::Corruption("invalid journal end marker".into()).into()),
    }
    let expected = reader.u64()?;
    let mut trailer = [0; 4];
    reader.bytes(&mut trailer)?;
    if trailer != *crate::file::MAGIC_BYTES {
        return Err(RecoveryError::Corruption("invalid journal trailer".into()).into());
    }
    if hasher.finish() != expected {
        return Err(RecoveryError::Corruption("journal batch checksum mismatch".into()).into());
    }
    Ok(PreparedBatch { seqno, items })
}

fn verify_zero_padding(reader: &mut Reader<'_>) -> Result<(), RecoveryError> {
    let end = reader.file.metadata().map_err(RecoveryError::Io)?.len();
    let mut position = reader.file.stream_position().map_err(RecoveryError::Io)?;
    if position == end {
        return Ok(());
    }
    const CHUNK: usize = 8192;
    reader.budget.charge(Kind::AllocationBytes, CHUNK as u64)?;
    let mut scratch = [0_u8; CHUNK];
    while position < end {
        let count = usize::try_from((end - position).min(CHUNK as u64))
            .map_err(|_| RecoveryError::Corruption("invalid padding length".into()))?;
        reader.budget.charge(Kind::InputBytes, count as u64)?;
        let chunk = scratch
            .get_mut(..count)
            .ok_or_else(|| RecoveryError::Corruption("invalid padding length".into()))?;
        reader.file.read_exact(chunk).map_err(RecoveryError::Io)?;
        if chunk.iter().any(|byte| *byte != 0) {
            return Err(RecoveryError::Corruption(
                "nonzero bytes after journal padding".into(),
            ));
        }
        position += count as u64;
    }
    Ok(())
}
