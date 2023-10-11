use std::collections::{HashMap, HashSet};

use crate::commons::channel::SenderEnd;
use crate::commons::models::state::Subject;
use crate::commons::self_signature_manager::{SelfSignatureInterface, SelfSignatureManager};
use crate::distribution::{AskForSignatures, SignaturesReceived};
use crate::governance::stage::ValidationStage;
use crate::identifier::{Derivable, DigestIdentifier, KeyIdentifier};
use crate::message::{MessageConfig, MessageTaskCommand};
use crate::protocol::protocol_message_manager::TapleMessages;
use crate::signature::Signature;
use crate::utils::message::distribution::{
    create_distribution_request, create_distribution_response,
};
use crate::utils::message::ledger::{request_gov_event, request_lce};
use crate::{
    database::{Error as DbError, DB},
    governance::GovernanceInterface,
    DatabaseCollection,
};
use crate::{Metadata, Settings, DigestDerivator};

use super::error::{DistributionErrorResponses, DistributionManagerError};
use super::StartDistribution;
pub struct InnerDistributionManager<G: GovernanceInterface, C: DatabaseCollection> {
    governance: G,
    db: DB<C>,
    messenger_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    signature_manager: SelfSignatureManager,
    timeout: u32,
    replication_factor: f64,
    derivator: DigestDerivator,
}

impl<G: GovernanceInterface, C: DatabaseCollection> InnerDistributionManager<G, C> {
    pub fn new(
        governance: G,
        db: DB<C>,
        messenger_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        signature_manager: SelfSignatureManager,
        settings: Settings,
        derivator: DigestDerivator,
    ) -> Self {
        Self {
            governance,
            db,
            messenger_channel,
            signature_manager,
            timeout: settings.node.timeout,
            replication_factor: settings.node.replication_factor,
            derivator
        }
    }

    pub async fn governance_updated(
        &self,
        governance_id: &DigestIdentifier,
    ) -> Result<(), DistributionManagerError> {
        let all_subjects_ids = self
            .db
            .get_subjects_by_governance(governance_id)
            .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
        let governance = self
            .db
            .get_subject(governance_id)
            .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
        // We have the IDs of the affected subjects. Whether we are still a witness or not will depend largely on the namespace and schema_id.
        for id in all_subjects_ids {
            let subject = self
                .db
                .get_subject(&id)
                .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
            let owner = subject.owner.clone();
            let metadata = build_metadata(&subject, governance.sn);
            let mut witnesses = self.get_targets(metadata, &subject).await?;
            if !witnesses.contains(&self.signature_manager.get_own_identifier()) {
                // We are no longer witnesses
                self.db
                    .del_witness_signatures(&id)
                    .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
                continue;
            }
            // We remain witnesses. We check to see if we are missing any signatures.
            let (_, current_signatures) = self
                .db
                .get_witness_signatures(&id)
                .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
            let current_signers: HashSet<KeyIdentifier> =
                current_signatures.into_iter().map(|s| s.signer).collect();
            let remaining_signers: HashSet<KeyIdentifier> =
                witnesses.difference(&current_signers).cloned().collect();
            witnesses.insert(owner);
            if !remaining_signers.is_empty() {
                self.send_signature_request(&id, subject.sn, witnesses, &remaining_signers)
                    .await?;
            }
        }
        Ok(())
    }

    async fn restart_distribution(
        &self,
        subject: &Subject,
        signatures: &HashSet<Signature>,
        witnesses: &HashSet<KeyIdentifier>,
    ) -> Result<(), DistributionManagerError> {
        let remaining_signatures = self.get_remaining_signers(signatures, witnesses);
        if remaining_signatures.len() > 0 {
            self.send_signature_request(
                &subject.subject_id,
                subject.sn,
                witnesses.clone(),
                &remaining_signatures,
            )
            .await?;
        }
        Ok(())
    }

    pub async fn init(&self) -> Result<(), DistributionManagerError> {
        // We need to check all the subjects that we know and check if we have all the signatures of
        // witnessing for their last event. These subjects will be from different governances, and for each one of
        // them the conditions will be different. It is thus a complex process.
        // However, in practice, the process can be simplified by simply resorting to the saved // witnessing signatures.
        // witnessing. If we have at least one signature, then we were testifying the subject. No
        // however, there is no way to avoid querying governance. As you might guess, it is necessary to implement
        // some kind of cache to avoid querying the same governance many times. An alternative is to group the subjects
        // by governance instead.
        let mut governances_version = HashMap::new();
        let signatures = self
            .db
            .get_all_witness_signatures()
            .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
        // We have the SubjectID, but we need the governance of each one of them, as well as the sn
        // Each subject must be requested separately.
        let mut governances_still_witness_flags: HashMap<
            (DigestIdentifier, String, String),
            (bool, Option<HashSet<KeyIdentifier>>),
        > = HashMap::new();
        for (subject_id, sn, signatures) in signatures.iter() {
            let subject = match self.db.get_subject(subject_id) {
                Ok(subject) => subject,
                Err(error) => match error {
                    DbError::EntryNotFound => {
                        // We do not know the subject we are receiving signatures from. We are not going to ask for anything
                        continue;
                    }
                    _ => return Err(DistributionManagerError::DatabaseError(error.to_string())),
                },
            };
            if sn != &subject.sn {
                // If the SN does not match then either the distribution process was not called or it was completed successfully, in either case the Ledger should re-request the operation again.
                // cases, the Ledger should re-request the operation.
                continue;
            }
            let schema_id = subject.schema_id.clone();
            let namespace = subject.namespace.clone();
            let governance_id = if &subject.schema_id == "governance" {
                subject.subject_id.clone()
            } else {
                subject.governance_id.clone()
            };
            // We check if we have already analyzed subjects of the same type previously.
            if let Some((node_is_witness, witnesses)) = governances_still_witness_flags.get(&(
                governance_id.clone(),
                schema_id.clone(),
                namespace.clone(),
            )) {
                if !node_is_witness {
                    // Ya no somos testigos. Descartamos las firmas.
                    self.db
                        .del_witness_signatures(subject_id)
                        .map_err(|error| {
                            DistributionManagerError::DatabaseError(error.to_string())
                        })?;
                    continue;
                } else {
                    // We are witnesses. We carry out the process of requesting signatures
                    // In principle, we already have our signature.
                    // It is possible that the witnesses have changed and some signatures are no longer correct. However,
                    // this is not a problem. Having extra signatures is irrelevant for protocol purposes.
                    self.restart_distribution(&subject, &signatures, witnesses.as_ref().unwrap())
                        .await?;
                    continue;
                }
            }
            // We have not previously analyzed the same combination of governance, schema and namespace.
            // We ask for the governance since we need to know its current version.
            let governance_version = {
                if &schema_id == "governance" {
                    governances_version.insert(governance_id.clone(), subject.sn);
                    subject.sn
                } else {
                    match governances_version.get(&subject.governance_id) {
                        Some(version) => *version,
                        None => {
                            let governance = match self.db.get_subject(&governance_id) {
                                Ok(subject) => subject,
                                Err(error) => match error {
                                    DbError::EntryNotFound => {
                                        // We do not know the subject we are receiving signatures from. We are not going to ask for anything
                                        continue;
                                    }
                                    _ => {
                                        return Err(DistributionManagerError::DatabaseError(
                                            error.to_string(),
                                        ))
                                    }
                                },
                            };
                            governances_version.insert(governance_id.clone(), governance.sn);
                            governance.sn
                        }
                    }
                }
            };
            // We check whether we are still witnesses for the subjects of this governance.
            let mut witnesses = self
                .get_targets(build_metadata(&subject, governance_version), &subject)
                .await?;
            if witnesses.contains(&self.signature_manager.get_own_identifier()) {
                // We continue to be witnesses
                witnesses.insert(subject.owner.clone());
                witnesses.remove(&self.signature_manager.get_own_identifier());
                self.restart_distribution(&subject, &signatures, &witnesses)
                    .await?;
                governances_still_witness_flags.insert(
                    (governance_id, schema_id, namespace),
                    (true, Some(witnesses)),
                );
            } else {
                // We are no longer witnesses
                // We can erase the signatures. We also point out that this combination of
                // governance + schema + namespace is invalid.
                self.db
                    .del_witness_signatures(subject_id)
                    .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
                governances_still_witness_flags
                    .insert((governance_id, schema_id, namespace), (false, None));
            }
        }
        Ok(())
    }

    pub async fn start_distribution(
        &self,
        msg: StartDistribution,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        // The ledger has asked us to start the distribution process.
        // First we should start by generating the signature of the event to be distributed.
        let event = match self.db.get_event(&msg.subject_id, msg.sn) {
            Ok(event) => event,
            Err(error) => match error {
                DbError::EntryNotFound => {
                    // We do not know the event we are receiving signatures from. We are not going to ask for anything
                    return Ok(Err(DistributionErrorResponses::EventNotFound(
                        msg.sn,
                        msg.subject_id.to_str(),
                    )));
                }
                _ => return Err(DistributionManagerError::DatabaseError(error.to_string())),
            }, // No debería ocurrir
        };
        let signature = self
            .signature_manager
            .sign(&event, self.derivator)
            .map_err(|_| DistributionManagerError::SignGenerarionFailed)?;
        // Delete the previous signatures before adding the new ones
        self.db
            .del_witness_signatures(&msg.subject_id)
            .map_err(|_| DistributionManagerError::SignGenerarionFailed)?;
        self.db
            .set_witness_signatures(&msg.subject_id, msg.sn, HashSet::from_iter(vec![signature]))
            .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
        let subject = self
            .db
            .get_subject(&msg.subject_id)
            .map_err(|e| DistributionManagerError::DatabaseError(e.to_string()))?;
        let owner = subject.owner.clone();
        let governance_version = self
            .governance
            .get_governance_version(subject.governance_id.clone(), subject.subject_id.clone())
            .await
            .map_err(|_| DistributionManagerError::GovernanceChannelNotAvailable)?;
        // We start distribution
        let metadata = build_metadata(&subject, governance_version);
        let mut targets = self.get_targets(metadata, &subject).await?;
        targets.insert(owner);
        targets.remove(&self.signature_manager.get_own_identifier());
        if !targets.is_empty() {
            self.send_signature_request(&subject.subject_id, msg.sn, targets.clone(), &targets)
                .await?;
        }
        Ok(Ok(()))
    }

    async fn cancel_signature_request(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<(), DistributionManagerError> {
        let subject_id_str = subject_id.to_str();
        self.messenger_channel
            .tell(MessageTaskCommand::Cancel(format!(
                "WITNESS/{}",
                subject_id_str
            )))
            .await
            .map_err(|_| DistributionManagerError::MessageChannelNotAvailable)?;
        Ok(())
    }

    async fn send_signature_request(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        targets: HashSet<KeyIdentifier>,
        signatures_requested: &HashSet<KeyIdentifier>,
    ) -> Result<(), DistributionManagerError> {
        let subject_id_str = subject_id.to_str();
        let request = create_distribution_request(
            subject_id.clone(),
            sn,
            signatures_requested.clone(),
            self.signature_manager.get_own_identifier(),
        );
        self.messenger_channel
            .tell(MessageTaskCommand::Request(
                Some(format!("WITNESS/{}", subject_id_str)),
                request,
                Vec::from_iter(targets.into_iter()),
                MessageConfig {
                    timeout: self.timeout,
                    replication_factor: self.replication_factor,
                },
            ))
            .await
            .map_err(|_| DistributionManagerError::MessageChannelNotAvailable)?;
        Ok(())
    }

    async fn get_targets(
        &self,
        metadata: Metadata,
        subject: &Subject,
    ) -> Result<HashSet<KeyIdentifier>, DistributionManagerError> {
        // Owner of subject must be included
        let owner = subject.owner.clone();
        let mut targets = self
            .governance
            .get_signers(metadata, ValidationStage::Witness)
            .await
            .map_err(|_| DistributionManagerError::GovernanceChannelNotAvailable)?;
        targets.insert(owner);
        Ok(targets)
    }

    pub async fn provide_signatures(
        &self,
        msg: &AskForSignatures,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        // Signatures are requested
        // We check if we have them
        match self.db.get_witness_signatures(&msg.subject_id) {
            Ok((sn, signatures)) => {
                // We check SN
                if sn == msg.sn {
                    // We give the signatures
                    let requested = &msg.signatures_requested;
                    let result = signatures
                        .iter()
                        .filter(|s| requested.contains(&s.signer))
                        .cloned()
                        .collect();
                    let response = create_distribution_response(msg.subject_id.clone(), sn, result);
                    self.messenger_channel
                        .tell(MessageTaskCommand::Request(
                            None,
                            response,
                            vec![msg.sender_id.clone()],
                            MessageConfig::direct_response(),
                        ))
                        .await
                        .map_err(|_| DistributionManagerError::MessageChannelNotAvailable)?;
                } else if msg.sn > sn {
                    // I don't see the need for a message for MSG.SN = SN + 1.
                    let request = if self
                        .governance
                        .is_governance(msg.subject_id.clone())
                        .await
                        .map_err(|_| DistributionManagerError::GovernanceChannelNotAvailable)?
                    {
                        request_gov_event(
                            self.signature_manager.get_own_identifier(),
                            msg.subject_id.clone(),
                            sn + 1,
                        )
                    } else {
                        request_lce(
                            self.signature_manager.get_own_identifier(),
                            msg.subject_id.clone(),
                        )
                    };
                    self.messenger_channel
                        .tell(MessageTaskCommand::Request(
                            None,
                            request,
                            vec![msg.sender_id.clone()],
                            MessageConfig::direct_response(),
                        ))
                        .await
                        .map_err(|_| DistributionManagerError::MessageChannelNotAvailable)?;
                }
            }
            Err(DbError::EntryNotFound) => {
                // Subject has no witnessing signatures.
                let request = request_lce(
                    self.signature_manager.get_own_identifier(),
                    msg.subject_id.clone(),
                );
                self.messenger_channel
                    .tell(MessageTaskCommand::Request(
                        None,
                        request,
                        vec![msg.sender_id.clone()],
                        MessageConfig::direct_response(),
                    ))
                    .await
                    .map_err(|_| DistributionManagerError::MessageChannelNotAvailable)?;
            }
            Err(error) => {
                return Err(DistributionManagerError::DatabaseError(error.to_string()));
            }
        }
        return Ok(Ok(()));
    }

    fn get_remaining_signers(
        &self,
        current_signatures: &HashSet<Signature>,
        targets: &HashSet<KeyIdentifier>,
    ) -> HashSet<KeyIdentifier> {
        let current_signers: HashSet<&KeyIdentifier> =
            current_signatures.iter().map(|s| &s.signer).collect();
        let targets_ref: HashSet<&KeyIdentifier> = targets.iter().map(|s| s).collect();
        targets_ref
            .difference(&current_signers)
            .map(|&s| s.clone())
            .collect()
    }

    pub async fn signatures_received(
        &self,
        msg: SignaturesReceived,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        // We receive witness signatures
        // We check the validity of the signatures and save them, and update the task.
        // We check if we have the subject and event to which the signatures belong.
        // In principle, if we have it, it is as simple as checking if we already have previous witnessing signatures.
        match self.db.get_witness_signatures(&msg.subject_id) {
            Ok((sn, current_signatures)) => {
                if msg.sn != sn {
                    return Ok(Err(DistributionErrorResponses::SignaturesNotFound));
                }
                // We check the signatures
                let event = match self.db.get_event(&msg.subject_id, msg.sn) {
                    Ok(event) => event,
                    Err(error) => match error {
                        DbError::EntryNotFound => {
                            // We do not know the event we are receiving signatures from. We are not going to ask for anything
                            return Ok(Err(DistributionErrorResponses::EventNotFound(
                                msg.sn,
                                msg.subject_id.to_str(),
                            )));
                        }
                        _ => {
                            return Err(DistributionManagerError::DatabaseError(error.to_string()))
                        }
                    },
                };
                let subject = match self.db.get_subject(&msg.subject_id) {
                    Ok(subject) => subject,
                    Err(error) => {
                        return Err(DistributionManagerError::DatabaseError(error.to_string()))
                    }
                };
                let owner = subject.owner.clone();
                let governance_version = self
                    .governance
                    .get_governance_version(
                        subject.governance_id.clone(),
                        subject.subject_id.clone(),
                    )
                    .await
                    .map_err(|_| DistributionManagerError::GovernanceChannelNotAvailable)?;
                let metadata = build_metadata(&subject, governance_version);
                let mut targets = self.get_targets(metadata, &subject).await?;
                for signature in msg.signatures.iter() {
                    // We check signer
                    if !targets.contains(&signature.signer) {
                        return Ok(Err(DistributionErrorResponses::InvalidSigner));
                    }
                    // We check signature
                    if let Err(_error) = signature.verify(&event) {
                        return Ok(Err(DistributionErrorResponses::InvalidSignature));
                    }
                }
                // The signatures are correct
                targets.remove(&self.signature_manager.get_own_identifier());
                let current_signatures: HashSet<Signature> =
                    current_signatures.union(&msg.signatures).cloned().collect();
                // We calculate missing signatures
                let remaining_signatures =
                    self.get_remaining_signers(&current_signatures, &targets);
                self.db
                    .set_witness_signatures(&msg.subject_id, msg.sn, current_signatures)
                    .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
                if remaining_signatures.len() == 0 {
                    self.cancel_signature_request(&subject.subject_id).await?;
                } else {
                    targets.insert(owner);
                    self.send_signature_request(
                        &subject.subject_id,
                        msg.sn,
                        targets,
                        &remaining_signatures,
                    )
                    .await?;
                }
                Ok(Ok(()))
            }
            Err(DbError::EntryNotFound) => {
                // We do not know the event we are receiving signatures from. We are not going to ask for anything
                return Ok(Ok(()));
            }
            Err(error) => return Err(DistributionManagerError::DatabaseError(error.to_string())),
        }
    }
}

fn build_metadata(subject: &Subject, governance_version: u64) -> Metadata {
    Metadata {
        namespace: subject.namespace.clone(),
        subject_id: subject.subject_id.clone(),
        governance_id: subject.governance_id.clone(),
        governance_version: governance_version,
        schema_id: subject.schema_id.clone(),
    }
}
