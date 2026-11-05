// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl<B: Backend + Clone> GroupKeyLane<B> {
    /// Replica writing under this group's Personae-derived key.
    pub fn for_identity<P: IdentityProvider + ?Sized>(
        backend: B,
        group: GroupSessionId,
        identity: &P,
    ) -> Result<Self, GroupKeyLaneError> {
        let salt = group_key_writer_salt(group);
        let keypair = identity.derive_keypair(&salt)?;
        let attestation = identity.attest_derived_key(&salt)?;
        let stable_author =
            stable_writer_subject(keypair.public_key().to_bytes(), Some(&attestation), &salt)?;
        if stable_author != identity.master_public_key().to_bytes() {
            return Err(GroupKeyLaneError::RootMismatch);
        }
        Ok(Self {
            store: MunimentStore::new(backend),
            group,
            signing_seed: keypair.to_seed(),
            stable_author,
            attestation,
            limits: GroupKeyLimits::default(),
        })
    }

    pub fn with_limits(mut self, limits: GroupKeyLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn group(&self) -> GroupSessionId {
        self.group
    }

    /// Stable Personae root this replica authors under.
    pub fn stable_author(&self) -> [u8; 32] {
        self.stable_author
    }

    pub fn sync_store(&self) -> MunimentStore<B, GroupKeyExt> {
        self.store.clone()
    }

    /// Give a receive task admission capability without the local signing seed.
    pub fn receiver(&self) -> GroupKeyReceiver<B> {
        GroupKeyReceiver {
            processor: self.processor(),
        }
    }

    pub(super) fn processor(&self) -> OperationProcessor<B, GroupKeyExt, GroupKeyPolicy> {
        OperationProcessor::new(
            self.store.clone(),
            GroupKeyPolicy {
                group: self.group,
                limits: self.limits,
            },
        )
    }

    /// Sign and store one dispatch the local session produced.
    ///
    /// The session applied the dispatch when it produced it, so persist the
    /// session before authoring: a record on the lane whose transition the
    /// author lost would fork the author's control sequence.
    pub async fn author_dispatch(
        &self,
        dispatch: &GroupSessionDispatch,
    ) -> Result<Operation<GroupKeyExt>, GroupKeyLaneError> {
        let entries = causal_entries(&load_operations(&self.store, self.group).await?);
        let parents = observed_frontier(&entries)?;
        let author = SigningKey::from_bytes(&self.signing_seed).verifying_key();
        let (seq_num, backlink) = author_head(&entries, *author.as_bytes(), &GROUP_KEY_LOG)?;
        let operation = self.build_operation(
            dispatch,
            self.attestation.clone(),
            parents,
            seq_num,
            backlink,
        )?;
        match self.accept(&operation).await? {
            GroupKeyAdmission::Inserted { .. } => Ok(operation),
            other => Err(GroupKeyLaneError::NotInserted(other)),
        }
    }

    pub(super) fn build_operation(
        &self,
        dispatch: &GroupSessionDispatch,
        author_attestation: DerivedKeyAttestation,
        parents: Vec<[u8; 32]>,
        seq_num: u32,
        backlink: Option<[u8; 32]>,
    ) -> Result<Operation<GroupKeyExt>, GroupKeyLaneError> {
        let record = GroupKeyRecord {
            version: GROUP_KEY_RECORD_VERSION,
            control: dispatch.control.to_bytes()?,
            direct: dispatch
                .direct
                .iter()
                .map(|frame| frame.to_bytes().map(ByteBuf::from))
                .collect::<Result<_, _>>()?,
            author_attestation,
        };
        let body_bytes = encode_cbor(&record).map_err(wire)?;
        let header = Header::builder()
            .body(&body_bytes)
            .seq_num(seq_num)
            .backlink(backlink.map(Hash::from))
            .build(
                &SigningKey::from_bytes(&self.signing_seed),
                GroupKeyExt {
                    group: self.group.0,
                    parents,
                },
            );
        Ok(Operation {
            hash: header.hash(),
            header,
            body: Some(Body::from_bytes(&body_bytes)),
        })
    }

    /// Verify and store one received record.
    pub async fn accept(
        &self,
        operation: &Operation<GroupKeyExt>,
    ) -> Result<GroupKeyAdmission, GroupKeyLaneError> {
        self.receiver().accept(operation).await
    }

    /// Apply every ready, unsettled frame to `session` in causal order.
    ///
    /// When anything was applied, `persist` receives the advanced session
    /// before the settled marks are written. The frames are applied to an
    /// in-memory working copy. If persistence fails, the caller's live session
    /// remains unchanged. An error can leave the save outcome uncertain, so the
    /// callback must make a same-session retry safe (for example, by saving
    /// idempotently), or the host must reload persisted state before retrying.
    /// A successful return means the advanced session is recoverable. The live
    /// session is then advanced before awaiting the settled-mark write, so it
    /// continues to match persisted state if that write fails or the future is
    /// cancelled.
    pub async fn drain_ready<S, F, E>(
        &self,
        session: &mut S,
        persist: F,
    ) -> Result<GroupKeyDrain, GroupKeyLaneError>
    where
        S: GroupKeySession,
        F: FnOnce(&S) -> Result<(), E>,
        E: fmt::Display,
    {
        if session.group() != self.group {
            return Err(GroupKeyLaneError::WrongGroup {
                expected: self.group,
                actual: session.group(),
            });
        }
        // Keep the caller's session transactional across persistence failure.
        // This codec copy contains secret state and remains in memory only.
        let mut working_session = session.try_clone()?;
        let operations = load_operations(&self.store, self.group).await?;
        let entries = causal_entries(&operations);
        let causal = causal_projection(&entries)?;
        let prefix = settled_prefix(self.group);
        let settled: BTreeSet<String> = self
            .store
            .backend()
            .list(&prefix)
            .await?
            .into_iter()
            .collect();
        let member = working_session.member();
        let mut drain = GroupKeyDrain::default();
        let mut blocked = BTreeSet::new();
        let mut marks = Vec::new();
        for index in causal.order {
            let entry = &entries[index];
            let key = format!("{prefix}{}", hex::encode(entry.operation));
            if settled.contains(&key) {
                continue;
            }
            if entry
                .backlink
                .iter()
                .chain(&entry.parents)
                .any(|dependency| blocked.contains(dependency))
            {
                blocked.insert(entry.operation);
                drain.pending.push(entry.operation);
                continue;
            }
            let frame = match open_record(&operations[index], self.group, &self.limits) {
                Err(reject) => SettledFrame::Refused(reject.to_string()),
                Ok((root, dispatch)) => {
                    match working_session.process_frame(
                        root,
                        &dispatch.control,
                        dispatch.direct_for(member),
                    ) {
                        Ok(process) => {
                            drain.applied.push(AppliedGroupFrame {
                                operation: entry.operation,
                                control: dispatch.control.id,
                                process,
                            });
                            SettledFrame::Applied
                        }
                        Err(GroupSessionError::UnexpectedSequence {
                            expected, actual, ..
                        }) if actual < expected => {
                            drain.superseded.push(entry.operation);
                            SettledFrame::Superseded
                        }
                        Err(
                            GroupSessionError::UnexpectedSequence { .. }
                            | GroupSessionError::MissingPrekey(_),
                        ) => {
                            blocked.insert(entry.operation);
                            drain.pending.push(entry.operation);
                            continue;
                        }
                        Err(error) => SettledFrame::Refused(error.to_string()),
                    }
                }
            };
            if let SettledFrame::Refused(reason) = &frame {
                drain.refused.push(RefusedGroupFrame {
                    operation: entry.operation,
                    reason: reason.clone(),
                });
            }
            let value = encode_cbor(&frame).map_err(wire)?;
            marks.push(WriteOp::Put { key, value });
        }
        drain
            .pending
            .extend(causal.pending.iter().map(|pending| pending.operation));
        for key in self
            .store
            .backend()
            .list(&parked_prefix(self.group))
            .await?
        {
            if let Some(bytes) = self.store.backend().get(&key).await? {
                drain.pending.push(*decode_parked(&bytes)?.hash.as_bytes());
            }
        }
        if !drain.applied.is_empty() {
            persist(&working_session)
                .map_err(|error| GroupKeyLaneError::Persist(error.to_string()))?;
            *session = working_session;
        }
        if !marks.is_empty() {
            self.store.backend().apply(&marks).await?;
        }
        Ok(drain)
    }
}
