use std::collections::{HashMap, HashSet};

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
        // Tenemos los IDs de los sujetos afectados. Si seguimos siendo testigos o no dependerá en gran medida del namespace y el schema_id
        for id in all_subjects_ids {
            let subject = self
                .db
                .get_subject(&id)
                .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
            let owner = subject.owner.clone();
            let metadata = build_metadata(&subject, governance.sn);
            let mut witnesses = self
                .governance
                .get_signers(metadata, ValidationStage::Witness)
                .await
                .map_err(|_| DistributionManagerError::GovernanceChannelNotAvailable)?;
            if !witnesses.contains(&self.signature_manager.get_own_identifier()) {
                // Ya no somos testigos
                self.db
                    .del_witness_signatures(&id)
                    .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
                continue;
            }
            // Seguimos siendo testigos. Comprobamos si nos falta alguna firma.
            let (_, current_signatures) = self
                .db
                .get_witness_signatures(&id)
                .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
            let current_signers: HashSet<KeyIdentifier> = current_signatures
                .into_iter()
                .map(|s| s.content.signer)
                .collect();
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
        log::warn!("REMAIMING SIGNATURES: {}", remaining_signatures.len());
        for signer in remaining_signatures.iter() {
            log::warn!("REMAIMING SIGNER: {}", signer.to_str());
        }
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
        // Tenemos que comprobar todos los sujetos que conocemos y comprobar si tenemos todas las firmas de
        // testificación para su último evento. Estos sujetos serán de diferentes gobernanzas, y para cada una de
        // ellas las condiciones serán diferentes. Se tata pues de un proceso complejo.
        // No obstante, en la práctica, el proceso se puede simplificar simplemente recurriendo a las firmas guardadas
        // de testificación. Si tenemos al menos una firma, entonces es que estábamos testificando el sujeto. No
        // obtante no hay manera de evitar el consultar la gobernanza. Como se puede suponer, es necesario implementar
        // algún tipo de caché para evitar consultar muchas veces la misma gobernanza. Una alterntiva es agrupar los sujetos
        // por gobernanzas en su lugar.
        let mut governances_version = HashMap::new();
        let signatures = self
            .db
            .get_all_witness_signatures()
            .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
        // Tenemos el SubjectID, pero necesitamos la gobernanza de cada uno de ellos, así como el sn
        // Se deberá de pedir cada sujeto por separado.
        let mut governances_still_witness_flags: HashMap<
            (DigestIdentifier, String, String),
            (bool, Option<HashSet<KeyIdentifier>>),
        > = HashMap::new();
        for (subject_id, sn, signatures) in signatures.iter() {
            let subject = self
                .db
                .get_subject(subject_id)
                .map_err(|error| DistributionManagerError::DatabaseError(error.to_string()))?;
            if sn != &subject.sn {
                // Si el SN no coincide es que o no se llamó el proceso de distribución o se completó con éxito, en cualquiera de los
                // casos, el Ledger debería volver a solicitar la operación
                continue;
            }
            let schema_id = subject.schema_id.clone();
            let namespace = subject.namespace.clone();
            let governance_id = if &subject.schema_id == "governance" {
                subject.subject_id.clone()
            } else {
                subject.governance_id.clone()
            };
            // Comprobamos si ya hemos analizado sujetos del mismo tipo previamente
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
                    // Somos testigos. Realizamos el proceso de solicitud de firmas
                    // En principio, ya tenemos nuestra firma.
                    // Es posible que los testigos hayan cambiado y algunas firmas ya no sean correctas. No obstante,
                    // esto no supone ningún problema. Tener firmas de más es irrelevante a nivel de protocolo.
                    self.restart_distribution(&subject, &signatures, witnesses.as_ref().unwrap())
                        .await?;
                    continue;
                }
            }
            // No hemos analizado previamente la misma combinación de gobernanza, schema y namespace
            // Pedimos la gobernanza ya que necesitamos saber su versión actual
            let governance_version = {
                if &schema_id == "governance" {
                    governances_version.insert(governance_id.clone(), subject.sn);
                    subject.sn
                } else {
                    match governances_version.get(&subject.governance_id) {
                        Some(version) => *version,
                        None => {
                            let governance =
                                self.db.get_subject(&governance_id).map_err(|error| {
                                    DistributionManagerError::DatabaseError(error.to_string())
                                })?;
                            governances_version.insert(governance_id.clone(), governance.sn);
                            governance.sn
                        }
                    }
                }
            };
            // Comprobamos si seguimos siendo testigos para los sujetos de esta gobernanza
            let mut witnesses = self
                .governance
                .get_signers(
                    build_metadata(&subject, governance_version),
                    ValidationStage::Witness,
                )
                .await
                .map_err(|_| DistributionManagerError::GovernanceChannelNotAvailable)?;
            if witnesses.contains(&self.signature_manager.get_own_identifier()) {
                // Seguimos siendo testigos
                witnesses.insert(subject.owner.clone());
                witnesses.remove(&self.signature_manager.get_own_identifier());
                self.restart_distribution(&subject, &signatures, &witnesses)
                    .await?;
                governances_still_witness_flags.insert(
                    (governance_id, schema_id, namespace),
                    (true, Some(witnesses)),
                );
            } else {
                // Ya no somos testigos
                // Podemos borrar la firmas. Indicamos además que esta combinación de
                // gobernanza + schema + namespace no es válida
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
        // Empezamos la distribución
        let metadata = build_metadata(&subject, governance_version);
        let mut targets = self.get_targets(metadata).await?;
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
    ) -> Result<HashSet<KeyIdentifier>, DistributionManagerError> {
        self.governance
            .get_signers(metadata, ValidationStage::Witness)
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
            }
            Err(DbError::EntryNotFound) => {
                // El sujeto no tiene firmas de testificación.
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
        let current_signers: HashSet<&KeyIdentifier> = current_signatures
            .iter()
            .map(|s| &s.content.signer)
            .collect();
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
                let mut targets = self.get_targets(metadata).await?;
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
                        .verify(&hash_signed.derivative(), &signature.signature)
                    {
                        return Ok(Err(DistributionErrorResponses::InvalidSignature));
                    }
                }
                // Las firmas son correctas
                targets.remove(&self.signature_manager.get_own_identifier());
                let current_signatures: HashSet<Signature> =
                    current_signatures.union(&msg.signatures).cloned().collect();
                // Calculamos firmas que nos faltan
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
