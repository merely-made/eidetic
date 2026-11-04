// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn payload_chunks_complete_a_header_in_the_same_drop() {
    pollster::block_on(async {
        let key = SigningKey::generate();
        let operation = operation(&key, 0, None);
        let body = operation.body.as_ref().unwrap().to_bytes();
        let digest = *operation.header.payload_hash.unwrap().as_bytes();
        let split = body.len() / 2;
        let records = vec![
            operation_record(&operation, false),
            DropRecord::PayloadChunk {
                payload_hash: digest,
                offset: split as u64,
                bytes: body[split..].to_vec(),
            },
            DropRecord::PayloadChunk {
                payload_hash: digest,
                offset: 0,
                bytes: body[..split].to_vec(),
            },
        ];
        let mut bytes = Vec::new();
        write_plain_drop(&mut bytes, &records, DropLimits::default()).unwrap();
        let processor = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);

        let report = import_plain_drop(
            std::io::Cursor::new(bytes),
            DropLimits::default(),
            &processor,
        )
        .await
        .unwrap();
        assert_eq!(report.accepted, 1);
        assert_eq!(
            processor
                .store()
                .get_operation(&operation.hash)
                .await
                .unwrap()
                .unwrap()
                .body
                .unwrap()
                .to_bytes(),
            body
        );
    });
}

#[test]
fn payload_only_drop_waits_for_its_header() {
    pollster::block_on(async {
        let key = SigningKey::generate();
        let operation = operation(&key, 0, None);
        let body = operation.body.as_ref().unwrap().to_bytes();
        let payload_hash = operation.header.payload_hash.unwrap();
        let chunk = DropRecord::PayloadChunk {
            payload_hash: *payload_hash.as_bytes(),
            offset: 0,
            bytes: body.clone(),
        };
        let mut payload_drop = Vec::new();
        write_plain_drop(&mut payload_drop, &[chunk], DropLimits::default()).unwrap();
        let processor = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
        let payload_report = import_plain_drop(
            std::io::Cursor::new(payload_drop),
            DropLimits::default(),
            &processor,
        )
        .await
        .unwrap();
        assert_eq!(payload_report.accepted, 0);
        assert_eq!(
            processor
                .store()
                .backend()
                .list("pending-payload/")
                .await
                .unwrap()
                .len(),
            1
        );

        let mut header_drop = Vec::new();
        write_plain_drop(
            &mut header_drop,
            &[operation_record(&operation, false)],
            DropLimits::default(),
        )
        .unwrap();
        let header_report = import_plain_drop(
            std::io::Cursor::new(header_drop),
            DropLimits::default(),
            &processor,
        )
        .await
        .unwrap();
        assert_eq!(header_report.accepted, 1);
        assert!(
            processor
                .store()
                .backend()
                .list("pending-payload/")
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            processor
                .store()
                .get_operation(&operation.hash)
                .await
                .unwrap()
                .unwrap()
                .body
                .unwrap()
                .to_bytes(),
            body
        );
    });
}

#[test]
fn header_only_drop_accepts_its_payload_later() {
    pollster::block_on(async {
        let key = SigningKey::generate();
        let operation = operation(&key, 0, None);
        let body = operation.body.as_ref().unwrap().to_bytes();
        let payload_hash = operation.header.payload_hash.unwrap();
        let processor = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);

        let mut header_drop = Vec::new();
        write_plain_drop(
            &mut header_drop,
            &[operation_record(&operation, false)],
            DropLimits::default(),
        )
        .unwrap();
        import_plain_drop(
            std::io::Cursor::new(header_drop),
            DropLimits::default(),
            &processor,
        )
        .await
        .unwrap();
        assert!(
            processor
                .store()
                .get_operation(&operation.hash)
                .await
                .unwrap()
                .unwrap()
                .body
                .is_none()
        );

        let mut payload_drop = Vec::new();
        write_plain_drop(
            &mut payload_drop,
            &[DropRecord::PayloadChunk {
                payload_hash: *payload_hash.as_bytes(),
                offset: 0,
                bytes: body.clone(),
            }],
            DropLimits::default(),
        )
        .unwrap();
        import_plain_drop(
            std::io::Cursor::new(payload_drop),
            DropLimits::default(),
            &processor,
        )
        .await
        .unwrap();
        assert_eq!(
            processor
                .store()
                .get_operation(&operation.hash)
                .await
                .unwrap()
                .unwrap()
                .body
                .unwrap()
                .to_bytes(),
            body
        );
    });
}

#[test]
fn blob_chunks_land_in_the_content_addressed_store() {
    pollster::block_on(async {
        let blob = b"native drop blob";
        let reference = proofs::BlobRef::blake3(blob);
        let digest = reference.digest.as_32().unwrap();
        let records = vec![
            DropRecord::BlobChunk {
                blob_hash: digest,
                offset: 7,
                bytes: blob[7..].to_vec(),
            },
            DropRecord::BlobChunk {
                blob_hash: digest,
                offset: 0,
                bytes: blob[..7].to_vec(),
            },
        ];
        let mut bytes = Vec::new();
        write_plain_drop(&mut bytes, &records, DropLimits::default()).unwrap();
        let processor = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
        import_plain_drop(
            std::io::Cursor::new(bytes),
            DropLimits::default(),
            &processor,
        )
        .await
        .unwrap();
        assert_eq!(
            processor.store().get_blob(&reference).await.unwrap(),
            Some(blob.to_vec())
        );
    });
}

#[test]
fn continuity_failure_keeps_a_durable_stage_and_commits_nothing_live() {
    pollster::block_on(async {
        let key = SigningKey::generate();
        let first = operation(&key, 0, None);
        let wrong = operation(
            &key,
            1,
            Some(p2panda_core::Hash::digest(b"wrong predecessor")),
        );
        let records = vec![
            operation_record(&first, true),
            operation_record(&wrong, true),
        ];
        let mut bytes = Vec::new();
        write_plain_drop(&mut bytes, &records, DropLimits::default()).unwrap();
        let processor = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);

        assert!(matches!(
            import_plain_drop(
                std::io::Cursor::new(bytes),
                DropLimits::default(),
                &processor,
            )
            .await,
            Err(DropIoError::Process(ProcessError::Backlink(_)))
        ));
        assert!(processor.store().is_empty().await.unwrap());
        assert_eq!(
            processor
                .store()
                .backend()
                .list("drop-receipt/")
                .await
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            processor
                .store()
                .backend()
                .list("drop-stage/")
                .await
                .unwrap()
                .len(),
            3
        );
        let staged = list_staged_drops(processor.store()).await.unwrap();
        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].record_count, 2);
        assert_eq!(
            discard_staged_drop(staged[0].drop_id, processor.store())
                .await
                .unwrap(),
            3
        );
        assert!(
            list_staged_drops(processor.store())
                .await
                .unwrap()
                .is_empty()
        );
    });
}

#[test]
fn staged_drop_resumes_after_its_predecessor_arrives() {
    pollster::block_on(async {
        let key = SigningKey::generate();
        let first = operation(&key, 0, None);
        let second = operation(&key, 1, Some(first.hash));
        let mut bytes = Vec::new();
        write_plain_drop(
            &mut bytes,
            &[operation_record(&second, true)],
            DropLimits::default(),
        )
        .unwrap();
        let processor = OperationProcessor::new(MunimentStore::new(MemoryBackend::new()), Policy);
        assert!(matches!(
            import_plain_drop(
                std::io::Cursor::new(bytes),
                DropLimits::default(),
                &processor,
            )
            .await,
            Err(DropIoError::Process(ProcessError::MissingPredecessor {
                seq_num: 1
            }))
        ));
        let staged = list_staged_drops(processor.store()).await.unwrap();
        assert_eq!(staged.len(), 1);

        processor.process(&first).await.unwrap();
        let report = resume_staged_drop(staged[0].drop_id, DropLimits::default(), &processor)
            .await
            .unwrap();
        assert_eq!(report.accepted, 1);
        assert!(
            list_staged_drops(processor.store())
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            processor
                .store()
                .get_operation(&second.hash)
                .await
                .unwrap()
                .is_some()
        );
    });
}

#[test]
fn effectful_drop_prunes_and_erases_in_the_same_receipted_commit() {
    pollster::block_on(async {
        let owner = SigningKey::generate();
        let first = operation(&owner, 0, None);
        let second = operation(&owner, 1, Some(first.hash));
        let victim_key = SigningKey::generate();
        let victim = operation(&victim_key, 0, None);
        let victim_body = victim.body.as_ref().unwrap().to_bytes();
        let victim_payload_hash = victim.header.payload_hash.unwrap();
        let backend = MemoryBackend::new();
        let processor = OperationProcessor::new(
            MunimentStore::new(backend),
            EffectPolicy { erase: victim.hash },
        );
        processor.process(&first).await.unwrap();
        processor.process(&second).await.unwrap();
        processor.process(&victim).await.unwrap();

        let checkpoint = operation(&owner, 2, Some(second.hash));
        let mut bytes = Vec::new();
        write_plain_drop(
            &mut bytes,
            &[operation_record(&checkpoint, true)],
            DropLimits::default(),
        )
        .unwrap();
        let report = import_plain_drop(
            std::io::Cursor::new(bytes),
            DropLimits::default(),
            &processor,
        )
        .await
        .unwrap();

        assert_eq!(report.accepted, 1);
        assert!(
            processor
                .store()
                .get_operation(&first.hash)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            processor
                .store()
                .get_operation(&second.hash)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            processor
                .store()
                .get_operation(&checkpoint.hash)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            processor
                .store()
                .get_operation(&victim.hash)
                .await
                .unwrap()
                .is_some_and(|operation| operation.body.is_none())
        );
        let mut late_payload_drop = Vec::new();
        write_plain_drop(
            &mut late_payload_drop,
            &[DropRecord::PayloadChunk {
                payload_hash: *victim_payload_hash.as_bytes(),
                offset: 0,
                bytes: victim_body,
            }],
            DropLimits::default(),
        )
        .unwrap();
        import_plain_drop(
            std::io::Cursor::new(late_payload_drop),
            DropLimits::default(),
            &processor,
        )
        .await
        .unwrap();
        assert!(
            processor
                .store()
                .get_operation(&victim.hash)
                .await
                .unwrap()
                .is_some_and(|operation| operation.body.is_none())
        );
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
            2
        );
    });
}
