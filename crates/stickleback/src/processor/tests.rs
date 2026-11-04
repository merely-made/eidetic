// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use muniment::MemoryBackend;
use p2panda_core::{Body, Header, SigningKey};
use p2panda_store::topics::TopicStore;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TestExt {
    space: u64,
}

#[derive(Clone, Copy)]
struct TestPolicy {
    space: u64,
    require_body: bool,
}

impl OperationPolicy<TestExt> for TestPolicy {
    type LogId = u64;

    fn admit(&self, operation: &Operation<TestExt>) -> Result<Admission<u64>, Reject> {
        if operation.header.extensions.space != self.space {
            return Err(Reject::new(
                "wrong-space",
                "operation addresses another space",
            ));
        }
        if self.require_body && operation.body.is_none() {
            return Err(Reject::new("missing-body", "test events require a body"));
        }
        Ok(Admission::keep(StoreTarget::new(
            Topic::from([self.space as u8; 32]),
            self.space,
        )))
    }
}

#[derive(Clone)]
struct RetentionPolicy {
    erase: p2panda_core::Hash,
}

impl OperationPolicy<TestExt> for RetentionPolicy {
    type LogId = u64;

    fn admit(&self, operation: &Operation<TestExt>) -> Result<Admission<u64>, Reject> {
        let target = StoreTarget::new(
            Topic::from([operation.header.extensions.space as u8; 32]),
            operation.header.extensions.space,
        );
        if operation.header.seq_num == 1 {
            // Repeating an authorized id must not inflate the erased
            // header count or apply the same erasure more than once.
            Ok(Admission::prune_before_current(target)
                .erasing_payloads([self.erase.clone(), self.erase.clone()]))
        } else {
            Ok(Admission::keep(target))
        }
    }
}

fn retention_processor(
    erase: &p2panda_core::Hash,
) -> OperationProcessor<MemoryBackend, TestExt, RetentionPolicy> {
    OperationProcessor::new(
        MunimentStore::new(MemoryBackend::new()),
        RetentionPolicy {
            erase: erase.clone(),
        },
    )
}

async fn retention_snapshot(
    processor: &OperationProcessor<MemoryBackend, TestExt, RetentionPolicy>,
) -> Vec<(String, Option<Vec<u8>>)> {
    let backend = processor.store().backend();
    let mut keys = Vec::new();
    for prefix in ["log/", "op/", "payload-ref/"] {
        keys.extend(backend.list(prefix).await.unwrap());
    }
    keys.sort();
    keys.dedup();
    let mut snapshot = Vec::new();
    for key in keys {
        let value = backend.get(&key).await.unwrap();
        snapshot.push((key, value));
    }
    snapshot
}

fn make_op(
    signing_key: &SigningKey,
    space: u64,
    seq_num: u32,
    backlink: Option<p2panda_core::Hash>,
) -> Operation<TestExt> {
    let body = Body::from_bytes(format!("event-{seq_num}").as_bytes());
    // p2panda 0.7.1 made the header's CBOR cache, size and digest private
    // and folded signing into the builder: `build` encodes, signs and
    // caches the digest in one step, so the struct-literal + `sign` pair
    // has no equivalent. `body` sets payload_size and payload_hash.
    let header = Header::builder()
        .body(body.as_bytes())
        .seq_num(seq_num)
        .backlink(backlink)
        .build(signing_key, TestExt { space });
    Operation {
        hash: header.hash(),
        header,
        body: Some(body),
    }
}

fn processor() -> OperationProcessor<MemoryBackend, TestExt, TestPolicy> {
    OperationProcessor::new(
        MunimentStore::new(MemoryBackend::new()),
        TestPolicy {
            space: 7,
            require_body: true,
        },
    )
}

fn header_processor() -> OperationProcessor<MemoryBackend, TestExt, TestPolicy> {
    OperationProcessor::new(
        MunimentStore::new(MemoryBackend::new()),
        TestPolicy {
            space: 7,
            require_body: false,
        },
    )
}

/// A checkpoint that both prunes sequence 0 and names its payload for
/// erasure must leave the same durable keys through either processor path.
/// Pruning removes the operation entirely, so it contributes one prune
/// and zero retained-payload erasures to the single-operation outcome.
#[test]
fn retention_single_and_batch_compose_prune_with_overlapping_erasure() {
    pollster::block_on(async {
        let signing_key = SigningKey::generate();
        let payload = make_op(&signing_key, 7, 0, None);
        let checkpoint = make_op(&signing_key, 7, 1, Some(payload.hash));

        let single = retention_processor(&payload.hash);
        assert!(single.process(&payload).await.unwrap().inserted());
        let single_outcome = single.process(&checkpoint).await.unwrap();

        let batch = retention_processor(&payload.hash);
        let outcome = batch
            .process_batch_atomic(&[payload.clone(), checkpoint.clone()], &[], &[])
            .await
            .unwrap();
        assert_eq!(
            retention_snapshot(&single).await,
            retention_snapshot(&batch).await
        );
        assert_eq!(outcome.inserted, 2);
        assert_eq!(outcome.duplicate, 0);
        assert_eq!(
            single_outcome,
            ProcessOutcome::Inserted {
                pruned_entries: 1,
                erased_payloads: 0,
            }
        );
        assert!(
            single
                .store()
                .get_operation(&checkpoint.hash)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            single
                .store()
                .get_operation(&checkpoint.hash)
                .await
                .unwrap()
                .unwrap()
                .body
                .is_some()
        );
        assert!(
            single
                .store()
                .get_operation(&payload.hash)
                .await
                .unwrap()
                .is_none()
        );
    });
}

/// An erase target in another log survives prefix pruning and is stripped
/// exactly once in both paths. BatchProcessOutcome intentionally reports
/// admission totals only; retention counts remain on ProcessOutcome.
#[test]
fn retention_single_and_batch_erase_payload_outside_pruned_prefix() {
    pollster::block_on(async {
        let signing_key = SigningKey::generate();
        let outside = make_op(&signing_key, 8, 0, None);
        let base = make_op(&signing_key, 7, 0, None);
        let checkpoint = make_op(&signing_key, 7, 1, Some(base.hash));

        let single = retention_processor(&outside.hash);
        assert!(single.process(&outside).await.unwrap().inserted());
        assert!(single.process(&base).await.unwrap().inserted());
        assert_eq!(
            single.process(&checkpoint).await.unwrap(),
            ProcessOutcome::Inserted {
                pruned_entries: 1,
                erased_payloads: 1,
            }
        );

        let batch = retention_processor(&outside.hash);
        assert!(batch.process(&outside).await.unwrap().inserted());
        let outcome = batch
            .process_batch_atomic(&[base, checkpoint.clone()], &[], &[])
            .await
            .unwrap();
        assert_eq!(outcome.inserted, 2);
        assert_eq!(outcome.duplicate, 0);
        assert_eq!(
            retention_snapshot(&single).await,
            retention_snapshot(&batch).await
        );
        let retained = single
            .store()
            .get_operation(&outside.hash)
            .await
            .unwrap()
            .unwrap();
        assert!(retained.body.is_none());
        assert!(
            single
                .store()
                .get_operation(&checkpoint.hash)
                .await
                .unwrap()
                .is_some()
        );
    });
}

#[test]
fn accepts_once_and_indexes_atomically() {
    pollster::block_on(async {
        let processor = processor();
        let signing_key = SigningKey::generate();
        let operation = make_op(&signing_key, 7, 0, None);

        assert_eq!(
            processor.process(&operation).await.unwrap(),
            ProcessOutcome::Inserted {
                pruned_entries: 0,
                erased_payloads: 0,
            }
        );
        assert_eq!(
            processor.process(&operation).await.unwrap(),
            ProcessOutcome::Duplicate
        );
        assert_eq!(processor.store().operation_count().await.unwrap(), 1);

        let resolved: std::collections::BTreeMap<_, Vec<u64>> = processor
            .store()
            .resolve(&Topic::from([7u8; 32]))
            .await
            .unwrap();
        assert_eq!(resolved.get(&signing_key.verifying_key()), Some(&vec![7]));
    });
}

#[test]
fn duplicate_full_operation_hydrates_a_retained_header() {
    pollster::block_on(async {
        let processor = header_processor();
        let full = make_op(&SigningKey::generate(), 7, 0, None);
        let mut header_only = full.clone();
        header_only.body = None;

        assert!(processor.process(&header_only).await.unwrap().inserted());
        assert_eq!(
            processor.process(&full).await.unwrap(),
            ProcessOutcome::HydratedPayload
        );
        assert!(
            processor
                .store()
                .get_operation(&full.hash)
                .await
                .unwrap()
                .is_some_and(|operation| operation.body.is_some())
        );
    });
}

#[test]
fn atomic_batch_reports_payload_hydration_of_a_duplicate_header() {
    pollster::block_on(async {
        let processor = header_processor();
        let full = make_op(&SigningKey::generate(), 7, 0, None);
        let mut header_only = full.clone();
        header_only.body = None;
        processor.process(&header_only).await.unwrap();

        let outcome = processor
            .process_batch_atomic(&[full.clone()], &[], &[])
            .await
            .unwrap();
        assert_eq!(outcome.inserted, 0);
        assert_eq!(outcome.duplicate, 1);
        assert_eq!(outcome.hydrated_payloads, 1);
        assert!(
            processor
                .store()
                .get_operation(&full.hash)
                .await
                .unwrap()
                .is_some_and(|operation| operation.body.is_some())
        );
    });
}

/// The staging key a lane retires and the operation it stores commit
/// together, and a refusal applies neither: the record stays staged
/// instead of being stored and left staged by a crash in between.
#[test]
fn process_with_writes_applies_the_insert_and_the_writes_together() {
    pollster::block_on(async {
        let processor = processor();
        let signing_key = SigningKey::generate();
        let backend = processor.store().backend().clone();
        let staged = |operation: &Operation<TestExt>| WriteOp::Delete {
            key: format!("staged/{}", operation.hash.to_hex()),
        };
        let stage = async |operation: &Operation<TestExt>| {
            let WriteOp::Delete { key } = staged(operation) else {
                unreachable!()
            };
            backend.put(&key, b"parked").await.unwrap();
            key
        };

        let refused = make_op(&signing_key, 9, 0, None);
        let refused_key = stage(&refused).await;
        assert!(matches!(
            processor
                .process_with_writes(&refused, &[staged(&refused)])
                .await,
            Err(ProcessError::Rejected(Reject {
                code: "wrong-space",
                ..
            }))
        ));
        assert!(processor.store().is_empty().await.unwrap());
        assert!(
            backend.get(&refused_key).await.unwrap().is_some(),
            "a refused batch leaves its writes unapplied"
        );

        let operation = make_op(&signing_key, 7, 0, None);
        let key = stage(&operation).await;
        assert!(
            processor
                .process_with_writes(&operation, &[staged(&operation)])
                .await
                .unwrap()
        );
        assert!(
            processor
                .store()
                .get_operation(&operation.hash)
                .await
                .unwrap()
                .is_some()
        );
        assert!(backend.get(&key).await.unwrap().is_none());

        // The same record offered twice: stored once, and the writes of the
        // second call still commit.
        let again = stage(&operation).await;
        assert!(
            !processor
                .process_with_writes(&operation, &[staged(&operation)])
                .await
                .unwrap()
        );
        assert!(backend.get(&again).await.unwrap().is_none());
        assert_eq!(processor.store().operation_count().await.unwrap(), 1);
    });
}

#[test]
fn policy_rejection_leaves_the_store_unchanged() {
    pollster::block_on(async {
        let processor = processor();
        let operation = make_op(&SigningKey::generate(), 9, 0, None);

        assert!(matches!(
            processor.process(&operation).await,
            Err(ProcessError::Rejected(Reject {
                code: "wrong-space",
                ..
            }))
        ));
        assert!(processor.store().is_empty().await.unwrap());
        let resolved: std::collections::BTreeMap<_, Vec<u64>> = processor
            .store()
            .resolve(&Topic::from([7u8; 32]))
            .await
            .unwrap();
        assert!(resolved.is_empty());
    });
}

#[test]
fn an_invalid_operation_leaves_the_store_unchanged() {
    pollster::block_on(async {
        let processor = processor();
        let mut operation = make_op(&SigningKey::generate(), 7, 0, None);
        // Mutating `extensions` no longer invalidates anything: since
        // p2panda 0.7.1 the header re-encodes from its CBOR cache and the
        // digest is cached too. Swapping the body is still caught, by the
        // payload-commitment half of `validate_operation`.
        operation.body = Some(Body::from_bytes(b"not the signed payload"));

        assert!(matches!(
            processor.process(&operation).await,
            Err(ProcessError::InvalidOperation(_))
        ));
        assert!(processor.store().is_empty().await.unwrap());
    });
}

#[test]
fn mismatched_operation_id_leaves_the_store_unchanged() {
    pollster::block_on(async {
        let processor = processor();
        let mut operation = make_op(&SigningKey::generate(), 7, 0, None);
        operation.hash = p2panda_core::Hash::digest(b"not-the-signed-header");

        assert!(matches!(
            processor.process(&operation).await,
            Err(ProcessError::InvalidOperation(_))
        ));
        assert!(processor.store().is_empty().await.unwrap());
    });
}

#[test]
fn enforces_predecessor_and_backlink_continuity() {
    pollster::block_on(async {
        let processor = processor();
        let signing_key = SigningKey::generate();
        let root = make_op(&signing_key, 7, 0, None);
        let next = make_op(&signing_key, 7, 1, Some(root.hash));

        assert!(matches!(
            processor.process(&next).await,
            Err(ProcessError::MissingPredecessor { seq_num: 1 })
        ));
        assert_eq!(
            processor.process(&root).await.unwrap(),
            ProcessOutcome::Inserted {
                pruned_entries: 0,
                erased_payloads: 0,
            }
        );
        assert_eq!(
            processor.process(&next).await.unwrap(),
            ProcessOutcome::Inserted {
                pruned_entries: 0,
                erased_payloads: 0,
            }
        );

        let fork = make_op(&signing_key, 7, 2, Some(root.hash));
        assert!(matches!(
            processor.process(&fork).await,
            Err(ProcessError::Backlink(_))
        ));
        assert_eq!(processor.store().operation_count().await.unwrap(), 2);
    });
}

#[test]
fn concurrent_forks_share_one_serialized_author_frontier() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(async {
        let processor = processor();
        let signing_key = SigningKey::generate();
        let left = make_op(&signing_key, 7, 0, None);
        let body = Body::from_bytes(b"different-root");
        // p2panda 0.7.1 made the header's CBOR cache, size and digest private
        // and folded signing into the builder: `build` encodes, signs and
        // caches the digest in one step, so the struct-literal + `sign` pair
        // has no equivalent. `body` sets payload_size and payload_hash.
        let header = Header::builder()
            .body(body.as_bytes())
            .seq_num(0)
            .backlink(None)
            .build(&signing_key, TestExt { space: 7 });
        let right = Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        };
        let left_processor = processor.clone();
        let right_processor = processor.clone();
        let left_task = tokio::spawn(async move { left_processor.process(&left).await });
        let right_task = tokio::spawn(async move { right_processor.process(&right).await });
        let left = left_task.await.unwrap();
        let right = right_task.await.unwrap();

        assert_eq!(
            usize::from(left.as_ref().is_ok_and(|outcome| outcome.inserted()))
                + usize::from(right.as_ref().is_ok_and(|outcome| outcome.inserted())),
            1
        );
        assert!(
            matches!(left, Err(ProcessError::StaleOperation { .. }))
                || matches!(right, Err(ProcessError::StaleOperation { .. }))
        );
        assert_eq!(processor.store().operation_count().await.unwrap(), 1);
    });
}
