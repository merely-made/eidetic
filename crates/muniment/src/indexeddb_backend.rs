// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The browser's durable byte store, as a [`Backend`].
//!
//! IndexedDB is the one storage API every browser exposes with a durable,
//! transactional, origin-scoped guarantee, which is the contract [`Backend`]
//! asks for. It sits beside [`RedbBackend`](crate::RedbBackend) and
//! [`ZipBackend`](crate::ZipBackend) as the third host realization: redb for
//! the desktop, zip for the portable archive, this for the browser.
//!
//! Keys are flat strings. `list` and `scan` narrow the fetch with an
//! `IDBKeyRange` over `getAllKeys` when the bounds are safe (see
//! [`key_range`](crate::key_range): IndexedDB orders strings by UTF-16 code
//! units, Muniment by code point, and they agree only for bounds below
//! U+D800), and otherwise fall back to enumerating every key. Either way the
//! result is post-filtered with Rust's comparison, so it is identical.
//!
//! Writes go through a transaction that this awaits to completion, so `put`,
//! `delete`, and `apply` resolve only once the browser has committed. `apply`
//! puts every op in ONE transaction, which is what makes a multi-key write
//! atomic: a tab closed mid-batch leaves the previous state, not half of it.
//!
//! # Version changes
//!
//! When another page opens the same database at a higher version, the browser
//! fires `versionchange` here. This connection closes itself so the upgrade
//! can proceed, and every later operation fails with a clear [`StoreError`]
//! ("database closed for a newer version; reopen"); hosts poll
//! [`IndexedDbBackend::is_closed_for_upgrade`] to reload. Conversely, if
//! `open` itself is blocked by another connection holding an older version,
//! it fails immediately with a typed error rather than waiting (an open
//! request that is blocked can wait indefinitely); the abandoned request
//! closes its connection if the browser later completes it. Browser behavior
//! here is not covered by native unit tests; only the pure state is.

use std::cell::RefCell;
use std::rc::Rc;

use async_trait::async_trait;
use futures_channel::oneshot;
use js_sys::{Array, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::{Closure, JsValue};
use web_sys::{
    Event, IdbDatabase, IdbFactory, IdbKeyRange, IdbObjectStore, IdbRequest, IdbTransaction, IdbTransactionMode,
};

use std::collections::HashMap;

use crate::backend::{TransactFn, TransactionReader};
use crate::idb_lifecycle::{ClosedFlag, blocked_error};
use crate::key_range::{KeyBounds, prefix_bounds, scan_bounds};
use crate::{Backend, StoreError, WriteOp};

const DATABASE_VERSION: u32 = 1;

/// A cloneable handle to one IndexedDB object store.
#[derive(Clone)]
pub struct IndexedDbBackend {
    database: IdbDatabase,
    store_name: String,
    read_only: bool,
    closed: ClosedFlag,
    /// Keeps the `onversionchange` closure alive for the life of the
    /// connection (shared by clones) and unregisters it on drop.
    _version_guard: Rc<VersionChangeGuard>,
}

struct VersionChangeGuard {
    database: IdbDatabase,
    _closure: Closure<dyn FnMut(Event)>,
}

impl Drop for VersionChangeGuard {
    fn drop(&mut self) {
        self.database.set_onversionchange(None);
    }
}

impl IndexedDbBackend {
    /// Open or create a database whose one object store contains flat Muniment
    /// byte keys.
    pub async fn open(database_name: &str, store_name: &str) -> Result<Self, StoreError> {
        let factory = indexed_db_factory()?;
        let request = factory
            .open_with_u32(database_name, DATABASE_VERSION)
            .map_err(js_backend_error)?;

        let upgrade_request = request.clone();
        let upgrade_store = store_name.to_string();
        let on_upgrade = Closure::<dyn FnMut(Event)>::new(move |_| {
            let Ok(value) = upgrade_request.result() else {
                return;
            };
            let Ok(database) = value.dyn_into::<IdbDatabase>() else {
                return;
            };
            if !database.object_store_names().contains(&upgrade_store) {
                let _ = database.create_object_store(&upgrade_store);
            }
        });
        request.set_onupgradeneeded(Some(on_upgrade.as_ref().unchecked_ref()));

        let value = match await_open(&request).await {
            Ok(value) => {
                request.set_onupgradeneeded(None);
                value
            },
            Err(error) => {
                // Blocked: the request is still pending and may yet fire
                // `upgradeneeded`/`success`, so its closures must outlive
                // this call. One-shot, rare path.
                if matches!(&error, StoreError::Backend(m) if m == crate::idb_lifecycle::OPEN_BLOCKED)
                {
                    on_upgrade.forget();
                    let late = Closure::<dyn FnMut(Event)>::new({
                        let late_request = request.clone();
                        move |_| {
                            let database = late_request
                                .result()
                                .ok()
                                .and_then(|value| value.dyn_into::<IdbDatabase>().ok());
                            if let Some(database) = database {
                                database.close();
                            }
                        }
                    });
                    request.set_onsuccess(Some(late.as_ref().unchecked_ref()));
                    late.forget();
                }
                return Err(error);
            },
        };
        let database = value
            .dyn_into::<IdbDatabase>()
            .map_err(|value| js_backend_error(value.into()))?;

        let closed = ClosedFlag::default();
        let version_closed = closed.clone();
        let version_database = database.clone();
        let on_version_change = Closure::<dyn FnMut(Event)>::new(move |_| {
            version_closed.mark_closed();
            version_database.close();
        });
        database.set_onversionchange(Some(on_version_change.as_ref().unchecked_ref()));
        Ok(Self {
            database: database.clone(),
            store_name: store_name.to_string(),
            read_only: false,
            closed,
            _version_guard: Rc::new(VersionChangeGuard {
                database,
                _closure: on_version_change,
            }),
        })
    }

    /// Whether this connection was closed because another page requested a
    /// newer database version. Once true every operation fails; the host
    /// should reload (or reopen) to pick up the new version.
    pub fn is_closed_for_upgrade(&self) -> bool {
        self.closed.is_closed()
    }

    /// This handle with every write refused. Reads are unchanged; `put`,
    /// `delete`, `apply` and `transact` return a [`StoreError`] without
    /// opening an IndexedDB transaction. The original handle is unaffected.
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    /// Whether this handle refuses writes.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    fn check_writable(&self) -> Result<(), StoreError> {
        self.closed.check()?;
        if self.read_only {
            Err(backend_error("this store handle is read-only"))
        } else {
            Ok(())
        }
    }

    fn transaction(
        &self,
        mode: IdbTransactionMode,
    ) -> Result<(IdbTransaction, IdbObjectStore), StoreError> {
        self.closed.check()?;
        let transaction = self
            .database
            .transaction_with_str_and_mode(&self.store_name, mode)
            .map_err(js_backend_error)?;
        let store = transaction
            .object_store(&self.store_name)
            .map_err(js_backend_error)?;
        Ok((transaction, store))
    }

    /// Keys inside `bounds` (a superset of the Rust-order answer; callers
    /// post-filter). Unbounded enumerates every key.
    async fn keys_in(&self, bounds: &KeyBounds) -> Result<Vec<String>, StoreError> {
        let (_transaction, store) = self.transaction(IdbTransactionMode::Readonly)?;
        let lower = bounds.lower.as_deref().map(JsValue::from_str);
        let upper = bounds.upper.as_deref().map(JsValue::from_str);
        let range = match (&lower, &upper) {
            (Some(l), Some(u)) => Some(IdbKeyRange::bound_with_lower_open_and_upper_open(
                l, u, false, true,
            )),
            (Some(l), None) => Some(IdbKeyRange::lower_bound(l)),
            (None, Some(u)) => Some(IdbKeyRange::upper_bound_with_open(u, true)),
            (None, None) => None,
        };
        let request = match range {
            Some(range) => {
                let range = range.map_err(js_backend_error)?;
                store.get_all_keys_with_key(&range)
            },
            None => store.get_all_keys(),
        }
        .map_err(js_backend_error)?;
        let value = await_request(&request).await?;
        Ok(Array::from(&value)
            .iter()
            .filter_map(|key| key.as_string())
            .collect())
    }

    /// Every key and its bytes, fetched from an already-open `store`. Used by
    /// [`transact`](Backend::transact) to snapshot the whole object store
    /// before calling the caller's closure: `get_all_keys`/`get_all` are two
    /// requests against the same store within the same transaction, so both
    /// see the state the transaction opened with, before any of this call's
    /// own writes land.
    async fn all_entries(store: &IdbObjectStore) -> Result<HashMap<String, Vec<u8>>, StoreError> {
        let keys_request = store.get_all_keys().map_err(js_backend_error)?;
        let keys_value = await_request(&keys_request).await?;
        let values_request = store.get_all().map_err(js_backend_error)?;
        let values_value = await_request(&values_request).await?;

        let keys: Vec<String> = Array::from(&keys_value)
            .iter()
            .filter_map(|key| key.as_string())
            .collect();
        let values = Array::from(&values_value);
        let mut entries = HashMap::with_capacity(keys.len());
        for (key, value) in keys.into_iter().zip(values.iter()) {
            let bytes = Uint8Array::new(&value);
            let mut output = vec![0; bytes.length() as usize];
            bytes.copy_to(&mut output);
            entries.insert(key, output);
        }
        Ok(entries)
    }
}

/// An in-memory snapshot of an object store, fetched up front so
/// [`IndexedDbBackend::transact`]'s closure can read synchronously against a
/// fixed point in time rather than awaiting a JS promise per read.
struct SnapshotReader(HashMap<String, Vec<u8>>);

impl TransactionReader for SnapshotReader {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.0.get(key).cloned())
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        Ok(self
            .0
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}

fn indexed_db_factory() -> Result<IdbFactory, StoreError> {
    if let Some(window) = web_sys::window() {
        return window
            .indexed_db()
            .map_err(js_backend_error)?
            .ok_or_else(|| backend_error("IndexedDB is unavailable"));
    }

    let global = js_sys::global();
    let worker = global
        .dyn_into::<web_sys::WorkerGlobalScope>()
        .map_err(|_| backend_error("browser window or worker scope is unavailable"))?;
    worker
        .indexed_db()
        .map_err(js_backend_error)?
        .ok_or_else(|| backend_error("IndexedDB is unavailable"))
}

#[async_trait(?Send)]
impl Backend for IndexedDbBackend {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let (_transaction, store) = self.transaction(IdbTransactionMode::Readonly)?;
        let request = store
            .get(&JsValue::from_str(key))
            .map_err(js_backend_error)?;
        let value = await_request(&request).await?;
        if value.is_undefined() {
            return Ok(None);
        }
        let bytes = Uint8Array::new(&value);
        let mut output = vec![0; bytes.length() as usize];
        bytes.copy_to(&mut output);
        Ok(Some(output))
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.check_writable()?;
        let (transaction, store) = self.transaction(IdbTransactionMode::Readwrite)?;
        let value = Uint8Array::from(bytes);
        store
            .put_with_key(value.as_ref(), &JsValue::from_str(key))
            .map_err(js_backend_error)?;
        await_transaction(&transaction).await
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.check_writable()?;
        let (transaction, store) = self.transaction(IdbTransactionMode::Readwrite)?;
        store
            .delete(&JsValue::from_str(key))
            .map_err(js_backend_error)?;
        await_transaction(&transaction).await
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        Ok(self
            .keys_in(&prefix_bounds(prefix))
            .await?
            .into_iter()
            .filter(|key| key.starts_with(prefix))
            .collect())
    }

    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        let Some(bounds) = scan_bounds(start, end) else {
            return Ok(Vec::new());
        };
        let mut keys = self
            .keys_in(&bounds)
            .await?
            .into_iter()
            .filter(|key| key.as_str() >= start && key.as_str() < end)
            .collect::<Vec<_>>();
        keys.sort();
        Ok(keys)
    }

    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        self.check_writable()?;
        if ops.is_empty() {
            return Ok(());
        }
        let (transaction, store) = self.transaction(IdbTransactionMode::Readwrite)?;
        for op in ops {
            match op {
                WriteOp::Put { key, value } => {
                    let bytes = Uint8Array::from(value.as_slice());
                    store
                        .put_with_key(bytes.as_ref(), &JsValue::from_str(key))
                        .map_err(js_backend_error)?;
                },
                WriteOp::Delete { key } => {
                    store
                        .delete(&JsValue::from_str(key))
                        .map_err(js_backend_error)?;
                },
            }
        }
        await_transaction(&transaction).await
    }

    /// Honest: one `Readwrite` transaction. `get_all_keys`/`get_all` (the
    /// gets) resolve into an in-memory snapshot before `f` runs, and the
    /// `put`/`delete` requests `f`'s writes describe are then issued against
    /// the *same* still-open transaction, before this awaits its
    /// `oncomplete`. No JS event-loop turn separates the reads from the
    /// writes' commit, so nothing else can interleave: an IndexedDB
    /// transaction only auto-closes once control returns to the browser event
    /// loop with no pending request, which never happens here.
    async fn transact(&self, f: TransactFn) -> Result<(), StoreError> {
        self.check_writable()?;
        let (transaction, store) = self.transaction(IdbTransactionMode::Readwrite)?;
        let snapshot = SnapshotReader(Self::all_entries(&store).await?);
        let ops = f(&snapshot);
        for op in ops {
            match op {
                WriteOp::Put { key, value } => {
                    let bytes = Uint8Array::from(value.as_slice());
                    store
                        .put_with_key(bytes.as_ref(), &JsValue::from_str(&key))
                        .map_err(js_backend_error)?;
                },
                WriteOp::Delete { key } => {
                    store
                        .delete(&JsValue::from_str(&key))
                        .map_err(js_backend_error)?;
                },
            }
        }
        await_transaction(&transaction).await
    }
}

/// Await an open request. Unlike [`await_request`] this also resolves on
/// `blocked` (with a typed error), so a held older connection cannot hang
/// the caller.
async fn await_open(request: &IdbRequest) -> Result<JsValue, StoreError> {
    let open: &web_sys::IdbOpenDbRequest = request.unchecked_ref();
    let (sender, receiver) = oneshot::channel::<Result<(), StoreError>>();
    let sender = Rc::new(RefCell::new(Some(sender)));
    let success_sender = sender.clone();
    let success = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Some(sender) = success_sender.borrow_mut().take() {
            let _ = sender.send(Ok(()));
        }
    });
    let blocked_sender = sender.clone();
    let blocked = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Some(sender) = blocked_sender.borrow_mut().take() {
            let _ = sender.send(Err(blocked_error()));
        }
    });
    let error_sender = sender;
    let error_request = request.clone();
    let error = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Some(sender) = error_sender.borrow_mut().take() {
            let message = error_request
                .error()
                .ok()
                .flatten()
                .map(|error| error.message())
                .unwrap_or_else(|| "IndexedDB request failed".to_string());
            let _ = sender.send(Err(backend_error(message)));
        }
    });
    request.set_onsuccess(Some(success.as_ref().unchecked_ref()));
    request.set_onerror(Some(error.as_ref().unchecked_ref()));
    open.set_onblocked(Some(blocked.as_ref().unchecked_ref()));
    let outcome = receiver
        .await
        .map_err(|_| backend_error("IndexedDB request was cancelled"))?;
    request.set_onsuccess(None);
    request.set_onerror(None);
    open.set_onblocked(None);
    outcome?;
    request.result().map_err(js_backend_error)
}

async fn await_request(request: &IdbRequest) -> Result<JsValue, StoreError> {
    let (sender, receiver) = oneshot::channel::<Result<(), StoreError>>();
    let sender = Rc::new(RefCell::new(Some(sender)));
    let success_sender = sender.clone();
    let success = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Some(sender) = success_sender.borrow_mut().take() {
            let _ = sender.send(Ok(()));
        }
    });
    let error_sender = sender;
    let error_request = request.clone();
    let error = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Some(sender) = error_sender.borrow_mut().take() {
            let message = error_request
                .error()
                .ok()
                .flatten()
                .map(|error| error.message())
                .unwrap_or_else(|| "IndexedDB request failed".to_string());
            let _ = sender.send(Err(backend_error(message)));
        }
    });
    request.set_onsuccess(Some(success.as_ref().unchecked_ref()));
    request.set_onerror(Some(error.as_ref().unchecked_ref()));
    let outcome = receiver
        .await
        .map_err(|_| backend_error("IndexedDB request was cancelled"))?;
    request.set_onsuccess(None);
    request.set_onerror(None);
    outcome?;
    request.result().map_err(js_backend_error)
}

async fn await_transaction(transaction: &IdbTransaction) -> Result<(), StoreError> {
    let (sender, receiver) = oneshot::channel::<Result<(), StoreError>>();
    let sender = Rc::new(RefCell::new(Some(sender)));
    let complete_sender = sender.clone();
    let complete = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Some(sender) = complete_sender.borrow_mut().take() {
            let _ = sender.send(Ok(()));
        }
    });
    let failed_sender = sender.clone();
    let failed_transaction = transaction.clone();
    let failed = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Some(sender) = failed_sender.borrow_mut().take() {
            let message = failed_transaction
                .error()
                .map(|error| error.message())
                .unwrap_or_else(|| "IndexedDB transaction failed".to_string());
            let _ = sender.send(Err(backend_error(message)));
        }
    });
    let aborted_sender = sender;
    let aborted_transaction = transaction.clone();
    let aborted = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Some(sender) = aborted_sender.borrow_mut().take() {
            let message = aborted_transaction
                .error()
                .map(|error| error.message())
                .unwrap_or_else(|| "IndexedDB transaction was aborted".to_string());
            let _ = sender.send(Err(backend_error(message)));
        }
    });
    transaction.set_oncomplete(Some(complete.as_ref().unchecked_ref()));
    transaction.set_onerror(Some(failed.as_ref().unchecked_ref()));
    transaction.set_onabort(Some(aborted.as_ref().unchecked_ref()));
    let outcome = receiver
        .await
        .map_err(|_| backend_error("IndexedDB transaction was cancelled"))?;
    transaction.set_oncomplete(None);
    transaction.set_onerror(None);
    transaction.set_onabort(None);
    outcome
}

fn backend_error(message: impl Into<String>) -> StoreError {
    StoreError::Backend(message.into())
}

fn js_backend_error(value: JsValue) -> StoreError {
    backend_error(value.as_string().unwrap_or_else(|| format!("{value:?}")))
}
