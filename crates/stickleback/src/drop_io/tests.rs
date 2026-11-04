// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::drop::write_plain_drop;
use crate::{
    Admission, DropReceiptLimits, Reject, StoreTarget, read_drop_receipt, write_drop_receipt,
};
use muniment::MemoryBackend;
use p2panda_core::{SigningKey, Topic};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Ext {
    space: u8,
}

#[derive(Clone, Copy)]
struct Policy;

impl OperationPolicy<Ext> for Policy {
    type LogId = u64;

    fn admit(&self, operation: &Operation<Ext>) -> Result<Admission<u64>, Reject> {
        if operation.header.extensions.space != 7 {
            return Err(Reject::new("wrong-space", "not this space"));
        }
        Ok(Admission::keep(StoreTarget::new(Topic::from([7; 32]), 0)))
    }
}

struct RadioSelector;

impl DropExportSelector<Ext> for RadioSelector {
    fn select(&self, operation: &Operation<Ext>) -> DropExportDecision {
        match operation.header.seq_num {
            0 => DropExportDecision::Omit,
            1 => DropExportDecision::Header { priority: 100 },
            _ => DropExportDecision::Full { priority: 10 },
        }
    }
}

struct TestProtector;

impl DropProtector for TestProtector {
    fn suite_id(&self) -> u16 {
        1
    }

    fn protect(&self, plaintext: &[u8], _aad: &[u8]) -> Result<Vec<u8>, NativeDropError> {
        Ok(plaintext.to_vec())
    }

    fn unprotect(
        &self,
        protected: &[u8],
        _aad: &[u8],
        _max_plaintext_bytes: u64,
    ) -> Result<Vec<u8>, NativeDropError> {
        Ok(protected.to_vec())
    }
}

#[derive(Clone)]
struct EffectPolicy {
    erase: p2panda_core::Hash,
}

impl OperationPolicy<Ext> for EffectPolicy {
    type LogId = u64;

    fn admit(&self, operation: &Operation<Ext>) -> Result<Admission<u64>, Reject> {
        if operation.header.extensions.space != 7 {
            return Err(Reject::new("wrong-space", "not this space"));
        }
        let target = StoreTarget::new(Topic::from([7; 32]), 0);
        Ok(if operation.header.seq_num == 2 {
            Admission::prune_before_current(target).erasing_payloads([self.erase])
        } else {
            Admission::keep(target)
        })
    }
}

fn operation(key: &SigningKey, seq: u32, backlink: Option<p2panda_core::Hash>) -> Operation<Ext> {
    let body = Body::from_bytes(format!("event-{seq}").as_bytes());
    // p2panda 0.7.1 made the header's CBOR cache, size and digest private
    // and folded signing into the builder: `build` encodes, signs and
    // caches the digest in one step, so the struct-literal + `sign` pair
    // has no equivalent. `body` sets payload_size and payload_hash.
    let header = Header::builder()
        .body(body.as_bytes())
        .seq_num(seq)
        .backlink(backlink)
        .build(key, Ext { space: 7 });
    Operation {
        hash: header.hash(),
        header,
        body: Some(body),
    }
}

#[test]
fn export_import_is_idempotent_and_uses_the_processor() {
    pollster::block_on(async {
        let key = SigningKey::generate();
        let first = operation(&key, 0, None);
        let second = operation(&key, 1, Some(first.hash));
        let records = vec![
            operation_record(&second, true),
            operation_record(&first, true),
        ];
        let mut bytes = Vec::new();
        write_plain_drop(&mut bytes, &records, DropLimits::default()).unwrap();

        let processor = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
        let first_report = import_plain_drop(
            std::io::Cursor::new(&bytes),
            DropLimits::default(),
            &processor,
        )
        .await
        .unwrap();
        assert_eq!(first_report.accepted, 2);
        assert!(!first_report.receipt_hit);
        let second_report = import_plain_drop(
            std::io::Cursor::new(&bytes),
            DropLimits::default(),
            &processor,
        )
        .await
        .unwrap();
        assert_eq!(second_report.duplicate, 2);
        assert!(second_report.receipt_hit);
        assert_eq!(second_report.drop_id, first_report.drop_id);
        assert!(
            processor
                .store()
                .backend()
                .list("drop-stage/")
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            processor
                .store()
                .backend()
                .list("drop-receipt/")
                .await
                .unwrap()
                .len(),
            1
        );

        let exported = export_topic_operations::<_, _, u64>(
            processor.store(),
            &Topic::from([7; 32]),
            DropExportProfile {
                include_operation_bodies: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(exported.len(), 2);
        assert!(exported.iter().all(|record| matches!(
            record,
            DropRecord::Operation {
                inline_body: None,
                ..
            }
        )));
    });
}

#[test]
fn plaintext_file_moves_one_topic_between_fresh_profiles() {
    pollster::block_on(async {
        let key = SigningKey::generate();
        let first = operation(&key, 0, None);
        let second = operation(&key, 1, Some(first.hash));
        let source = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
        source.process(&first).await.unwrap();
        source.process(&second).await.unwrap();

        let path = std::env::temp_dir().join(format!(
            "murm-drop-file-{}-{}.meredrop",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, b"previous export").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let written = export_plain_topic_file::<_, _, u64>(
            &path,
            source.store(),
            &Topic::from([7; 32]),
            DropExportProfile::default(),
            DropLimits::default(),
        )
        .await
        .unwrap();
        assert!(written.total_bytes > 52);
        assert_ne!(std::fs::read(&path).unwrap(), b"previous export");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }

        let destination = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
        let first_import = import_plain_drop_file(&path, DropLimits::default(), &destination)
            .await
            .unwrap();
        assert_eq!(first_import.drop_id, written.id);
        assert_eq!(first_import.accepted, 2);
        assert_eq!(destination.store().operation_count().await.unwrap(), 2);

        let repeated = import_plain_drop_file(&path, DropLimits::default(), &destination)
            .await
            .unwrap();
        assert!(repeated.receipt_hit);
        assert_eq!(repeated.duplicate, 2);
        std::fs::remove_file(path).unwrap();
    });
}

#[test]
fn refused_file_exports_preserve_existing_destination() {
    pollster::block_on(async {
        let key = SigningKey::generate();
        let operation = operation(&key, 0, None);
        let source = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
        source.process(&operation).await.unwrap();

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("existing.meredrop");
        let sentinel = b"preserve this prior export";
        let mut limits = DropLimits::default();
        limits.max_protected_bytes = 0;

        std::fs::write(&path, sentinel).unwrap();
        assert!(
            export_plain_topic_file::<_, _, u64>(
                &path,
                source.store(),
                &Topic::from([7; 32]),
                DropExportProfile::default(),
                limits,
            )
            .await
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), sentinel);

        std::fs::write(&path, sentinel).unwrap();
        assert!(
            export_selected_plain_topic_file::<_, _, u64, _>(
                &path,
                source.store(),
                &Topic::from([7; 32]),
                &DropExportProfile::default(),
                DropExportBudget::default(),
                limits,
            )
            .await
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), sentinel);

        std::fs::write(&path, sentinel).unwrap();
        assert!(
            export_protected_topic_file::<_, _, u64, _, _>(
                &path,
                source.store(),
                &Topic::from([7; 32]),
                &DropExportProfile::default(),
                DropExportBudget::default(),
                limits,
                &TestProtector,
            )
            .await
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), sentinel);

        let entries = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec![std::ffi::OsString::from("existing.meredrop")]);
    });
}

#[test]
fn staged_write_failure_preserves_destination_and_cleans_temporary() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.meredrop");
    let sentinel = b"preserve this prior export";
    std::fs::write(&path, sentinel).unwrap();

    let result: Result<(), NativeDropError> = write_staged_drop(&path, |writer| {
        writer.write_all(b"partial replacement")?;
        Err(NativeDropError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "injected write failure",
        )))
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), sentinel);
    assert_eq!(
        std::fs::read_dir(directory.path()).unwrap().count(),
        1,
        "failed export should remove its staged sibling"
    );
}

#[test]
fn refused_publication_preserves_directory_destination_and_cleans_temporary() {
    let parent = tempfile::tempdir().unwrap();
    let destination = parent.path().join("occupied.meredrop");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("sentinel"), b"keep directory contents").unwrap();

    let result = write_staged_drop(&destination, |writer| {
        writer.write_all(b"complete staged export")?;
        Ok(())
    });
    assert!(
        result.is_err(),
        "renaming a file onto a directory must be refused"
    );
    assert_eq!(
        std::fs::read(destination.join("sentinel")).unwrap(),
        b"keep directory contents"
    );
    assert_eq!(
        std::fs::read_dir(parent.path()).unwrap().count(),
        1,
        "refused publication should remove its staged sibling"
    );
}

#[test]
fn domain_selector_enforces_privacy_priority_and_radio_budget() {
    pollster::block_on(async {
        let key = SigningKey::generate();
        let first = operation(&key, 0, None);
        let second = operation(&key, 1, Some(first.hash));
        let third = operation(&key, 2, Some(second.hash));
        let source = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
        source.process(&first).await.unwrap();
        source.process(&second).await.unwrap();
        source.process(&third).await.unwrap();

        let (unbounded, unbounded_stats) = export_topic_operations_selected::<_, _, u64, _>(
            source.store(),
            &Topic::from([7; 32]),
            &RadioSelector,
            DropExportBudget::default(),
        )
        .await
        .unwrap();
        assert_eq!(unbounded_stats.policy_omitted, 1);
        assert_eq!(unbounded_stats.selected_records, 2);
        let high_priority_bytes = encode_cbor(&unbounded[0]).unwrap().len() as u64;
        let high_priority = decode_operation_record::<Ext>(&unbounded[0])
            .unwrap()
            .unwrap();
        assert_eq!(high_priority.header.seq_num, 1);
        assert!(high_priority.body.is_none());

        let path = std::env::temp_dir().join(format!(
            "murm-radio-drop-{}-{}.meredrop",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let report = export_selected_plain_topic_file::<_, _, u64, _>(
            &path,
            source.store(),
            &Topic::from([7; 32]),
            &RadioSelector,
            DropExportBudget {
                max_selected_record_bytes: Some(high_priority_bytes),
            },
            DropLimits::default(),
        )
        .await
        .unwrap();
        assert_eq!(report.selection.selected_records, 1);
        assert_eq!(report.selection.policy_omitted, 1);
        assert_eq!(report.selection.budget_omitted, 1);

        let (_, selected) = read_plain_drop(
            BufReader::new(File::open(&path).unwrap()),
            DropLimits::default(),
        )
        .unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(
            decode_operation_record::<Ext>(&selected[0])
                .unwrap()
                .unwrap()
                .header
                .seq_num,
            1
        );
        std::fs::remove_file(path).unwrap();
    });
}

#[test]
fn completed_receipt_round_trips_and_stays_peer_scoped() {
    pollster::block_on(async {
        let key = SigningKey::generate();
        let operation = operation(&key, 0, None);
        let records = vec![operation_record(&operation, true)];
        let mut drop_bytes = Vec::new();
        let written = write_plain_drop(&mut drop_bytes, &records, DropLimits::default()).unwrap();

        let receiver = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
        import_plain_drop(
            std::io::Cursor::new(drop_bytes),
            DropLimits::default(),
            &receiver,
        )
        .await
        .unwrap();
        let local = local_drop_receipt(written.id, receiver.store())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(local.operation_records, 1);
        assert_eq!(local.non_operation_records, 0);

        let mut receipt_bytes = Vec::new();
        write_drop_receipt(&mut receipt_bytes, local, DropReceiptLimits::default()).unwrap();
        let exchanged =
            read_drop_receipt(&receipt_bytes[..], DropReceiptLimits::default()).unwrap();

        let sender =
            OperationProcessor::new(MunimentStore::<_, Ext>::new(MemoryBackend::new()), Policy);
        let peer = ReceiptPeer::from_authenticated_identity(b"receiver-session-identity");
        store_peer_drop_receipt(peer, exchanged, sender.store())
            .await
            .unwrap();
        assert_eq!(
            peer_drop_receipt(peer, written.id, sender.store())
                .await
                .unwrap(),
            Some(local)
        );
        assert_eq!(
            local_drop_receipt(written.id, sender.store())
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            discard_peer_drop_receipts(peer, sender.store())
                .await
                .unwrap(),
            1
        );
    });
}

#[test]
fn unauthorized_batch_is_rejected_before_any_operation_lands() {
    pollster::block_on(async {
        let key = SigningKey::generate();
        let allowed = operation(&key, 0, None);
        let denied_key = SigningKey::generate();
        // A header can no longer be mutated and re-signed, so the operation
        // is authored with the offending space directly — same end state.
        let denied_body = Body::from_bytes(b"event-0");
        let denied_header = Header::builder()
            .body(denied_body.as_bytes())
            .seq_num(0)
            .backlink(None)
            .build(&denied_key, Ext { space: 8 });
        let denied = Operation {
            hash: denied_header.hash(),
            header: denied_header,
            body: Some(denied_body),
        };
        let records = vec![
            operation_record(&allowed, true),
            operation_record(&denied, true),
        ];
        let mut bytes = Vec::new();
        write_plain_drop(&mut bytes, &records, DropLimits::default()).unwrap();
        let processor = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
        assert!(
            import_plain_drop(
                std::io::Cursor::new(bytes),
                DropLimits::default(),
                &processor,
            )
            .await
            .is_err()
        );
        assert!(processor.store().is_empty().await.unwrap());
    });
}

#[path = "tests/recovery.rs"]
mod recovery;
