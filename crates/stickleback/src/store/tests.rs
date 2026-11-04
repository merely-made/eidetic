// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use muniment::MemoryBackend;
use p2panda_core::{AnyOperation, Body, Header, Operation, SigningKey};

type Ext = ();

/// Compile-time proof the adapter meets `LogSync`'s store bound: the two sync
/// traits it reconciles against, plus `Clone + Send + 'static` over a real
/// backend. If any part regresses, this stops compiling.
fn _log_sync_ready<S>()
where
    S: LogStore<AnyOperation, VerifyingKey, u64, u32, Hash>
        + TopicStore<Topic, VerifyingKey, u64>
        + Clone
        + Send
        + 'static,
{
}

#[allow(dead_code)]
fn _assert_ready() {
    _log_sync_ready::<MunimentStore<MemoryBackend, Ext>>();
}

/// A signed operation for one author's log at `seq`, chained onto `backlink`.
fn make_op(sk: &SigningKey, seq: u32, backlink: Option<Hash>, payload: &[u8]) -> Operation<Ext> {
    let body = Body::from_bytes(payload);
    let header = Header::<Ext>::builder()
        .body(payload)
        .seq_num(seq)
        .backlink(backlink)
        .build(sk, ());
    Operation {
        hash: header.hash(),
        header,
        body: Some(body),
    }
}

#[test]
fn current_stored_operations_round_trip_through_the_v7_codec() {
    let sk = SigningKey::generate();
    let operation = make_op(&sk, 0, None, b"current");
    assert_eq!(
        decode_op::<Ext>(&encode_op(&operation).unwrap()).unwrap(),
        operation
    );
}

#[test]
fn legacy_operation_bytes_report_the_reauthoring_boundary() {
    let error = decode_op::<Ext>(&[0x81, 0x00]).unwrap_err();
    assert!(error.to_string().contains("re-author"));
    assert!(error.to_string().contains("fresh store"));
}

#[test]
fn round_trips_and_orders_a_log() {
    pollster::block_on(async {
        let store = MunimentStore::<_, Ext>::new(MemoryBackend::new());
        let sk = SigningKey::generate();
        let author = sk.verifying_key();
        let log_id = 0u64;

        let op0 = make_op(&sk, 0, None, b"zero");
        let op1 = make_op(&sk, 1, Some(op0.hash), b"one");

        // Insert-or-ignore: the first insert takes, the repeat is a no-op.
        assert!(
            store
                .insert_operation(&op0.hash, &op0, &log_id)
                .await
                .unwrap()
        );
        assert!(
            !store
                .insert_operation(&op0.hash, &op0, &log_id)
                .await
                .unwrap()
        );
        assert!(
            store
                .insert_operation(&op1.hash, &op1, &log_id)
                .await
                .unwrap()
        );

        // OperationStore: fetch by id round-trips header and body.
        let got = store.get_operation(&op0.hash).await.unwrap().unwrap();
        assert_eq!(got.hash, op0.hash);
        assert_eq!(got.body.unwrap().to_bytes(), b"zero");
        assert!(store.has_operation(&op1.hash).await.unwrap());
        assert!(!store.has_operation(&Hash::digest(b"absent")).await.unwrap());

        // LogStore: latest is the highest seq; entries return in seq order.
        let latest = store
            .get_latest_entry(&author, &log_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(latest.hash, op1.hash);

        let entries = store
            .get_log_entries(&author, &log_id, None, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0.hash, op0.hash);
        assert_eq!(entries[1].0.hash, op1.hash);
        // The second tuple element is the encoded header.
        assert_eq!(entries[0].1, op0.header.encode());

        let heights = <MunimentStore<MemoryBackend, Ext> as LogStore<
            Operation<Ext>,
            VerifyingKey,
            u64,
            u32,
            Hash,
        >>::get_log_heights(&store, &author, &[log_id])
        .await
        .unwrap()
        .unwrap();
        assert_eq!(heights.get(&log_id), Some(&1));

        let (count, _bytes) = <MunimentStore<MemoryBackend, Ext> as LogStore<
            Operation<Ext>,
            VerifyingKey,
            u64,
            u32,
            Hash,
        >>::get_log_size(&store, &author, &log_id, None, None)
        .await
        .unwrap()
        .unwrap();
        assert_eq!(count, 2);

        // Range: after seq 0 leaves only op1.
        let tail = store
            .get_log_entries(&author, &log_id, Some(0), None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].0.hash, op1.hash);
    });
}

#[test]
fn associates_and_resolves_a_topic() {
    pollster::block_on(async {
        let store = MunimentStore::<_, Ext>::new(MemoryBackend::new());
        let sk = SigningKey::generate();
        let author = sk.verifying_key();
        let topic = Topic::random();
        let log_id = 7u64;

        assert!(store.associate(&topic, &author, &log_id).await.unwrap());
        assert!(!store.associate(&topic, &author, &log_id).await.unwrap());

        let resolved = store.resolve(&topic).await.unwrap();
        assert_eq!(resolved.get(&author), Some(&vec![log_id]));

        assert!(store.remove(&topic, &author, &log_id).await.unwrap());
        let empty: BTreeMap<VerifyingKey, Vec<u64>> = store.resolve(&topic).await.unwrap();
        assert!(empty.is_empty());
    });
}

#[test]
fn prunes_below_a_sequence() {
    pollster::block_on(async {
        let store = MunimentStore::<_, Ext>::new(MemoryBackend::new());
        let sk = SigningKey::generate();
        let author = sk.verifying_key();
        let log_id = 0u64;

        let op0 = make_op(&sk, 0, None, b"zero");
        let op1 = make_op(&sk, 1, Some(op0.hash), b"one");
        let op2 = make_op(&sk, 2, Some(op1.hash), b"two");
        for op in [&op0, &op1, &op2] {
            store.insert_operation(&op.hash, op, &log_id).await.unwrap();
        }

        // Prune below seq 2: op0 and op1 go, op2 stays.
        let pruned = <MunimentStore<MemoryBackend, Ext> as LogStore<
            Operation<Ext>,
            VerifyingKey,
            u64,
            u32,
            Hash,
        >>::prune_entries(&store, &author, &log_id, &2)
        .await
        .unwrap();
        assert_eq!(pruned, 2);
        assert!(!store.has_operation(&op0.hash).await.unwrap());
        assert!(store.has_operation(&op2.hash).await.unwrap());

        let remaining = store
            .get_log_entries(&author, &log_id, None, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].0.hash, op2.hash);
    });
}

#[test]
fn blob_collection_keeps_only_domain_traced_references() {
    pollster::block_on(async {
        let store = MunimentStore::<_, Ext>::new(MemoryBackend::new());
        let keep = store.put_blob(b"live checkpoint").await.unwrap();
        let collect = store.put_blob(b"expired payload").await.unwrap();
        assert_eq!(
            store.get_blob(&collect).await.unwrap(),
            Some(b"expired payload".to_vec())
        );

        let report = store.collect_unreferenced_blobs([&keep]).await.unwrap();
        assert_eq!(
            report,
            BlobGcReport {
                examined: 2,
                collected: 1,
            }
        );
        assert_eq!(
            store.get_blob(&keep).await.unwrap(),
            Some(b"live checkpoint".to_vec())
        );
        assert_eq!(store.get_blob(&collect).await.unwrap(), None);
    });
}
