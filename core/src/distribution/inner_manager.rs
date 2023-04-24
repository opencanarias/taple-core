use std::collections::HashSet;

use crate::commons::channel::SenderEnd;
use crate::commons::models::state::Subject;
use crate::commons::self_signature_manager::{SelfSignatureInterface, SelfSignatureManager};
use crate::distribution::{AskForSignatures, SignaturesReceived};
use crate::event_content::Metadata;
use crate::governance::stage::ValidationStage;
use crate::identifier::{Derivable, DigestIdentifier, KeyIdentifier};
use crate::message::{MessageConfig, MessageTaskCommand};
use crate::protocol::protocol_message_manager::TapleMessages;
use crate::signature::Signature;
use crate::utils::message::distribution::{
    create_distribution_request, create_distribution_response,
};
use crate::utils::message::ledger::request_lce;
use crate::TapleSettings;
use crate::{
    database::{Error as DbError, DB},
    governance::GovernanceInterface,
    DatabaseManager,
};

use super::error::{DistributionErrorResponses, DistributionManagerError};
use super::StartDistribution;
pub struct InnerDistributionManager<G: GovernanceInterface, D: DatabaseManager> {
    governance: G,
    db: DB<D>,
    messenger_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    signature_manager: SelfSignatureManager,
    timeout: u32,
    replication_factor: f64,
}

impl<G: GovernanceInterface, D: DatabaseManager> InnerDistributionManager<G, D> {
    pub fn new(
        governance: G,
        db: DB<D>,
        messenger_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        signature_manager: SelfSignatureManager,
        settings: TapleSettings,
    ) -> Self {
        Self {
            governance,
            db,
            messenger_channel,
            signature_manager,
            timeout: settings.node.timeout,
            replication_factor: settings.node.replication_factor,
        }
    }

    pub async fn start_distribution(
        &self,
        msg: StartDistribution,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        // El ledger nos ha pedido que empecemos el proceso de distribución
        // Primero deberíamos empezar generando la firma del evento a distribuir
        let event = match self.db.get_event(&msg.subject_id, msg.sn) {
            Ok(event) => event,
            Err(error) => return Err(DistributionManagerError::DatabaseError(error.to_string())), // No debería ocurrir
        };
        let signature = self
            .signature_manager
            .sign(&event)
            .map_err(|_| DistributionManagerError::SignGenerarionFailed)?;
        // Borrar las firmas anteriores antes de poner las nuevas
        self.db
            .del_witness_signatures(&msg.subject_id)
            .map_err(|_| DistributionManagerError::SignGenerarionFailed)?;
        self.db
            .set_signatures(&msg.subject_id, msg.sn, HashSet::from_iter(vec![signature]))
            .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
        let subject = self
            .db
            .get_subject(&msg.subject_id)
            .map_err(|e| DistributionManagerError::DatabaseError(e.to_string()))?;
        let governance_version = self
            .governance
            .get_governance_version(subject.governance_id.clone())
            .await
            .map_err(|_| DistributionManagerError::GovernanceChannelNotAvailable)?;
        // Empezamos la distribución
        let metadata = build_metadata(&subject, governance_version);
        let mut targets = self.get_targets(&metadata).await?;
        targets.remove(&self.signature_manager.get_own_identifier());
        self.send_signature_request(&subject.subject_id, msg.sn, targets.clone(), &targets)
            .await?;
        self.db
            .del_witness_signatures(&msg.subject_id)
            .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
        Ok(Ok(()))
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
        metadata: &Metadata,
    ) -> Result<HashSet<KeyIdentifier>, DistributionManagerError> {
        self.governance
            .get_signers(&metadata, ValidationStage::Witness)
            .await
            .map_err(|_| DistributionManagerError::GovernanceChannelNotAvailable)
    }

    pub async fn provide_signatures(
        &self,
        msg: &AskForSignatures,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        // Se solicitan firmas
        // Comprobamos si las tenemos
        match self.db.get_witness_signatures(&msg.subject_id) {
            Ok((sn, signatures)) => {
                // Comprobamos SN
                if sn == msg.sn {
                    // Damos las firmas
                    let requested = &msg.signatures_requested;
                    let result = signatures
                        .iter()
                        .filter(|s| requested.contains(&s.content.signer))
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
                    // No veo necesario un mensaje para el caso de MSG.SN = SN + 1
                    let request = request_lce(
                        msg.subject_id.clone(),
                        self.signature_manager.get_own_identifier(),
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
            }
            Err(DbError::EntryNotFound) => {
                // El sujeto no tiene firmas de testificación.
                let request = request_lce(
                    msg.subject_id.clone(),
                    self.signature_manager.get_own_identifier(),
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

    pub async fn signatures_received(
        &self,
        msg: SignaturesReceived,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        // Se reciben firmas de testificación
        // Comprobamos la validez de las firmas y las guardamos, actualizando además la tarea
        // Comprobamos si tenemos el sujeto y evento al que pertenecen las firmas
        // En pricnipio, si lo tenemos es tan sencillo como comprobar si ya tenemos firmas de testificación previas
        match self.db.get_witness_signatures(&msg.subject_id) {
            Ok((sn, current_signatures)) => {
                if msg.sn != sn {
                    return Ok(Err(DistributionErrorResponses::SignaturesNotFound));
                }
                // Comprobamos las firmas
                let event = match self.db.get_event(&msg.subject_id, msg.sn) {
                    Ok(event) => event,
                    Err(error) => {
                        return Err(DistributionManagerError::DatabaseError(error.to_string()))
                    }
                };
                let subject = match self.db.get_subject(&msg.subject_id) {
                    Ok(subject) => subject,
                    Err(error) => {
                        return Err(DistributionManagerError::DatabaseError(error.to_string()))
                    }
                };
                let governance_version = self
                    .governance
                    .get_governance_version(subject.governance_id.clone())
                    .await
                    .map_err(|_| DistributionManagerError::GovernanceChannelNotAvailable)?;
                let metadata = build_metadata(&subject, governance_version);
                let mut targets = self.get_targets(&metadata).await?;
                let hash_signed = DigestIdentifier::from_serializable_borsh(&event)
                    .map_err(|_| DistributionManagerError::HashGenerationFailed)?;
                for signature in msg.signatures.iter() {
                    // Comprobamos signer
                    if !targets.contains(&signature.content.signer) {
                        return Ok(Err(DistributionErrorResponses::InvalidSigner));
                    }
                    // Comprobamos firma
                    if let Err(_error) = signature
                        .content
                        .signer
                        .verify(&hash_signed.derivative(), signature.signature.clone())
                    {
                        return Ok(Err(DistributionErrorResponses::InvalidSignature));
                    }
                }
                // Las firmas son correctas
                targets.remove(&self.signature_manager.get_own_identifier());
                let current_signatures: HashSet<Signature> =
                    current_signatures.union(&msg.signatures).cloned().collect();
                // Calculamos firmas que nos faltan
                let current_signers: HashSet<&KeyIdentifier> = current_signatures
                    .iter()
                    .map(|s| &s.content.signer)
                    .collect();
                let targets_ref: HashSet<&KeyIdentifier> = targets.iter().map(|s| s).collect();
                let remaining_signatures: HashSet<KeyIdentifier> = targets_ref
                    .difference(&current_signers)
                    .map(|&s| s.clone())
                    .collect();
                self.db
                    .set_witness_signatures(&msg.subject_id, msg.sn, current_signatures)
                    .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
                self.send_signature_request(
                    &subject.subject_id,
                    msg.sn,
                    targets,
                    &remaining_signatures,
                )
                .await?;
                Ok(Ok(()))
            }
            Err(DbError::EntryNotFound) => {
                // No conocemos el evento del que nos llegan firmas. No vamos a pedir nada
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
        owner: subject.owner.clone(),
        creator: subject.creator.clone(),
    }
}
