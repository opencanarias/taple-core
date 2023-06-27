use std::collections::{HashMap, HashSet};

use crate::{
    commons::{
        channel::SenderEnd,
        errors::ChannelErrors,
        models::validation::ValidationProof,
        self_signature_manager::{SelfSignatureInterface, SelfSignatureManager},
    },
    event::EventCommand,
    event_content::Metadata,
    governance::{stage::ValidationStage, GovernanceAPI, GovernanceInterface},
    identifier::DigestIdentifier,
    message::{MessageConfig, MessageTaskCommand},
    protocol::protocol_message_manager::TapleMessages,
    signature::Signature,
    Derivable, KeyIdentifier,
};

use super::{errors::NotaryError, NotaryEvent, NotaryEventResponse};
use crate::database::{DatabaseCollection, DB};

pub struct Notary<C: DatabaseCollection> {
    gov_api: GovernanceAPI,
    database: DB<C>,
    signature_manager: SelfSignatureManager,
    message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
}

impl<C: DatabaseCollection> Notary<C> {
    pub fn new(
        gov_api: GovernanceAPI,
        database: DB<C>,
        signature_manager: SelfSignatureManager,
        message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    ) -> Self {
        Self {
            gov_api,
            database,
            signature_manager,
            message_channel,
        }
    }

    pub async fn notary_event(
        &self,
        notary_event: NotaryEvent,
        sender: KeyIdentifier,
    ) -> Result<NotaryEventResponse, NotaryError> {
        let actual_gov_version =
            if &notary_event.proof.schema_id == "governance" && notary_event.proof.sn == 0 {
                0
            } else {
                match self
                    .gov_api
                    .get_governance_version(
                        notary_event.proof.governance_id.clone(),
                        notary_event.proof.subject_id.clone(),
                    )
                    .await
                {
                    Ok(gov_version) => gov_version,
                    Err(error) => match error {
                        crate::governance::error::RequestError::GovernanceNotFound(_)
                        | crate::governance::error::RequestError::SubjectNotFound
                        | crate::governance::error::RequestError::InvalidGovernanceID => {
                            return Err(NotaryError::GovernanceNotFound);
                        }
                        crate::governance::error::RequestError::ChannelClosed => {
                            return Err(NotaryError::ChannelError(ChannelErrors::ChannelClosed));
                        }
                        _ => return Err(NotaryError::GovApiUnexpectedResponse),
                    },
                }
            };
        if actual_gov_version < notary_event.proof.governance_version {
            return Err(NotaryError::GovernanceVersionTooHigh);
        } else if actual_gov_version > notary_event.proof.governance_version {
            // Informar de Gov desactualizada
            self.message_channel
                .tell(MessageTaskCommand::Request(
                    None,
                    TapleMessages::EventMessage(
                        crate::event::EventCommand::HigherGovernanceExpected {
                            governance_id: notary_event.proof.governance_id.clone(),
                            who_asked: self.signature_manager.get_own_identifier(),
                        },
                    ),
                    vec![sender],
                    MessageConfig::direct_response(),
                ))
                .await?;
            return Err(NotaryError::GovernanceVersionTooLow);
        }
        let last_proof = {
            match self
                .database
                .get_notary_register(&notary_event.proof.subject_id)
            {
                Ok(last_proof) => Some(last_proof),
                Err(error) => match error {
                    crate::DbError::EntryNotFound => None,
                    _ => return Err(NotaryError::DatabaseError),
                },
            }
        };
        // Verificar firma de sujecto sobre proof
        let proof_hash = DigestIdentifier::from_serializable_borsh(&notary_event.proof)
            .map_err(|_| NotaryError::SubjectSignatureNotValid)?;
        if notary_event.subject_signature.verify(&notary_event.proof).is_err()
        {
            return Err(NotaryError::SubjectSignatureNotValid);
        }
        let subject_pk = self
            .check_proofs(
                &notary_event.proof,
                notary_event.previous_proof,
                notary_event.prev_event_validation_signatures,
                last_proof,
            )
            .await?;
        if notary_event.subject_signature.signer != subject_pk {
            return Err(NotaryError::SubjectSignatureNotValid);
        }
        self.database
            .set_notary_register(&notary_event.proof.subject_id, &notary_event.proof)
            .map_err(|_| NotaryError::DatabaseError)?;
        // Now we sign and send
        let notary_signature = self
            .signature_manager
            .sign(&notary_event.proof)
            .map_err(NotaryError::ProtocolErrors)?;
        log::warn!(
            "SE ENVÍA LA VALIDACIÓN A {}: sn: {}",
            sender.to_str(),
            notary_event.proof.sn
        );
        self.message_channel
            .tell(MessageTaskCommand::Request(
                None,
                TapleMessages::EventMessage(EventCommand::ValidatorResponse {
                    event_hash: notary_event.proof.event_hash,
                    signature: notary_signature.clone(),
                    governance_version: actual_gov_version,
                }),
                vec![sender],
                MessageConfig::direct_response(),
            ))
            .await?;
        Ok(NotaryEventResponse {
            notary_signature,
            gov_version_notary: actual_gov_version,
        })
    }

    async fn check_proofs(
        &self,
        new_proof: &ValidationProof,
        previous_proof: Option<ValidationProof>,
        validation_signatures: HashSet<Signature>,
        last_proof: Option<ValidationProof>,
    ) -> Result<KeyIdentifier, NotaryError> {
        match last_proof {
            Some(last_proof) => {
                log::warn!("TENGO LAST PROOF: {:?}", last_proof);
                // Comprobar que tenemos la prueba del evento anterior, si no tenemos que hacer la comprobación de la que nos llega en el mensaje como cuando no tenemos el registro
                if last_proof.sn > new_proof.sn {
                    Err(NotaryError::EventSnLowerThanLastSigned)
                } else if last_proof.sn == new_proof.sn && last_proof.sn != 0 {
                    // Comprobar que solo cambia la versión de la governanza
                    if !last_proof.is_similar(&new_proof) {
                        Err(NotaryError::DifferentProofForEvent)
                    } else {
                        Ok(last_proof.subject_public_key)
                    }
                } else if last_proof.sn + 1 == new_proof.sn {
                    if previous_proof.is_none() {
                        return Err(NotaryError::PreviousProofLeft);
                    }
                    // Comprobar que es similar a la prueba del evento anterior que nos llega en el mensaje
                    if !last_proof.is_similar(&previous_proof.unwrap()) {
                        Err(NotaryError::DifferentProofForEvent)
                    } else {
                        self.validate_previous_proof(new_proof, last_proof, None)
                            .await
                    }
                } else {
                    // Mismo caso que en not found, no tengo la prueba anterior
                    if new_proof.sn == 0 {
                        // Comprobar que es exactamente la misma, no se puede cambiar la gov version y no el subject_id, porque éste último depende de ella
                        if &last_proof != new_proof {
                            Err(NotaryError::DifferentProofForEvent)
                        } else {
                            Ok(new_proof.subject_public_key.clone())
                        }
                    } else {
                        if previous_proof.is_none() {
                            return Err(NotaryError::PreviousProofLeft);
                        }
                        self.validate_previous_proof(
                            new_proof,
                            previous_proof.unwrap(),
                            Some(validation_signatures),
                        )
                        .await
                    }
                }
            }
            None => {
                // Comprobar la prueba de validación anterior junto con las firmas de validación de dicha prueba, su validez criptográfica y si llega a quorum
                if previous_proof.is_none() && new_proof.sn != 0 {
                    return Err(NotaryError::PreviousProofLeft);
                } else if new_proof.sn != 0 {
                    self.validate_previous_proof(
                        new_proof,
                        previous_proof.unwrap(),
                        Some(validation_signatures),
                    )
                    .await
                } else {
                    if new_proof.governance_version != new_proof.genesis_governance_version {
                        return Err(NotaryError::GenesisGovVersionsDoesNotMatch(
                            new_proof.subject_id.to_str(),
                        ));
                    }
                    Ok(new_proof.subject_public_key.clone())
                }
            }
        }
    }

    async fn validate_previous_proof(
        &self,
        new_proof: &ValidationProof,
        previous_proof: ValidationProof,
        validation_signatures: Option<HashSet<Signature>>,
    ) -> Result<KeyIdentifier, NotaryError> {
        // Comprobar que la previous encaja con la nueva
        // TODO: Comprobar los demás campos, como subject_id, namespace...
        if previous_proof.event_hash != new_proof.prev_event_hash {
            return Err(NotaryError::DifferentProofForEvent);
        }
        if previous_proof.sn + 1 != new_proof.sn {
            return Err(NotaryError::DifferentProofForEvent);
        }
        if previous_proof.genesis_governance_version != new_proof.genesis_governance_version {
            return Err(NotaryError::DifferentProofForEvent);
        }
        if previous_proof.namespace != new_proof.namespace {
            return Err(NotaryError::DifferentProofForEvent);
        }
        if previous_proof.name != new_proof.name {
            return Err(NotaryError::DifferentProofForEvent);
        }
        if previous_proof.subject_id != new_proof.subject_id {
            return Err(NotaryError::DifferentProofForEvent);
        }
        if previous_proof.schema_id != new_proof.schema_id {
            return Err(NotaryError::DifferentProofForEvent);
        }
        if previous_proof.governance_id != new_proof.governance_id {
            return Err(NotaryError::DifferentProofForEvent);
        }
        match validation_signatures {
            Some(validation_signatures) => {
                let actual_signers: Result<HashSet<KeyIdentifier>, NotaryError> =
                    validation_signatures
                        .into_iter()
                        .map(|signature| {
                            if signature.verify(&previous_proof).is_err() {
                                return Err(NotaryError::InvalidSignature);
                            }
                            Ok(signature.signer)
                        })
                        .collect();
                let actual_signers = actual_signers?;
                let (signers, quorum_size) = self
                    .get_signers_and_quorum(
                        previous_proof.get_metadata(),
                        ValidationStage::Validate,
                    )
                    .await?;
                if !actual_signers.is_subset(&signers) {
                    return Err(NotaryError::InvalidSigner);
                }
                if actual_signers.len() < quorum_size as usize {
                    return Err(NotaryError::QuorumNotReached);
                }
            }
            None => {}
        }
        Ok(previous_proof.subject_public_key)
    }

    async fn get_signers_and_quorum(
        &self,
        metadata: Metadata,
        stage: ValidationStage,
    ) -> Result<(HashSet<KeyIdentifier>, u32), NotaryError> {
        let signers = self
            .gov_api
            .get_signers(metadata.clone(), stage.clone())
            .await
            .map_err(NotaryError::GovernanceError)?;
        let quorum_size = self
            .gov_api
            .get_quorum(metadata, stage)
            .await
            .map_err(NotaryError::GovernanceError)?;
        Ok((signers, quorum_size))
    }
}

/*
#[cfg(test)]
mod tests {
    use crate::commons::models::event::ValidationProof;
    use crate::commons::self_signature_manager::SelfSignatureManager;
    use crate::database::{MemoryCollection, MemoryManager, DB};
    use crate::governance::GovernanceUpdatedMessage;
    use crate::identifier::derive::SignatureDerivator;
    use crate::protocol::protocol_message_manager::TapleMessages;
    use crate::{
        commons::{
            channel::MpscChannel,
            crypto::{generate, Ed25519KeyPair},
            identifier::DigestIdentifier,
            models::{state::Subject, timestamp::TimeStamp},
        },
        governance::{
            governance::Governance, GovernanceAPI, GovernanceMessage, GovernanceResponse,
        },
        identifier::{KeyIdentifier, SignatureIdentifier},
        notary::{errors::NotaryError, NotaryEvent},
        signature::{Signature, SignatureContent},
        DigestDerivator,
    };
    use std::str::FromStr;
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    use super::Notary;

    #[test]
    fn test_all_good() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut gov, notary) = initialize();
            tokio::spawn(async move {
                gov.start().await;
            });
            let not_ev = not_ev(0);
            let result = notary.notary_event(not_ev).await;
            assert!(result.is_ok());
        })
    }

    /*
    #[test]
    fn test_gov_not_found() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut gov, notary) = initialize();
            tokio::spawn(async move {
                gov.start().await;
            });
            let mut not_ev = not_ev(0);
            not_ev.gov_id =
                DigestIdentifier::from_str("Jg2Nuv5bNs4swQGcPQ1CXs9MtcfwMVoeQDR2Ea1YNYJw").unwrap();
            let result = notary.notary_event(not_ev).await;
            assert!(result.is_err());
            let error = result.unwrap_err();
            assert_eq!(error, NotaryError::GovernanceNotFound)
        })
    }

    #[test]
    fn test_sn_too_small() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut gov, notary) = initialize();
            tokio::spawn(async move {
                gov.start().await;
            });
            let mut not_ev = not_ev(0);
            not_ev.subject_id =
                DigestIdentifier::from_str("Jg2Nuv5bNs4swQGcPQ1CXs9MtcfwMVoeQDR2Ea1YNYJw").unwrap();
            let result = notary.notary_event(not_ev).await;
            assert!(result.is_err());
            let error = result.unwrap_err();
            assert_eq!(error, NotaryError::EventSnLowerThanLastSigned)
        })
    }

    #[test]
    fn test_diff_hash() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut gov, notary) = initialize();
            tokio::spawn(async move {
                gov.start().await;
            });
            let mut not_ev = not_ev(3);
            not_ev.subject_id =
                DigestIdentifier::from_str("Jg2Nuv5bNs4swQGcPQ1CXs9MtcfwMVoeQDR2Ea1YNYJw").unwrap();
            not_ev.event_hash =
                DigestIdentifier::from_str("JKXo-EvPxQcL_nhbd4iprzyjdNxT9YYrmeJ7p5N_IVrg").unwrap();
            let result = notary.notary_event(not_ev).await;
            assert!(result.is_err());
            let error = result.unwrap_err();
            assert_eq!(error, NotaryError::DifferentHashForEvent)
        })
    }

    #[test]
    fn test_gov_version_too_high() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut gov, notary) = initialize();
            tokio::spawn(async move {
                gov.start().await;
            });
            let mut not_ev = not_ev(0);
            not_ev.gov_version = 4;
            let result = notary.notary_event(not_ev).await;
            assert!(result.is_err());
            let error = result.unwrap_err();
            assert_eq!(error, NotaryError::GovernanceVersionTooHigh)
        })
    }
    */

    fn initialize() -> (
        Governance<MemoryManager, MemoryCollection>,
        Notary<MemoryCollection>,
    ) {
        let manager = MemoryManager::new();
        let manager = Arc::new(manager);
        let db = DB::new(manager.clone());
        let subject = Subject {
            subject_id: DigestIdentifier::from_str("JKXo-EvPxQcL_nhbd4iprzyjdNxT9YYrmeJ7p5N_IVrg")
                .unwrap(),
            governance_id: DigestIdentifier::from_str("").unwrap(),
            sn: 0,
            public_key: KeyIdentifier::from_str("ED8MpwKh3OjPEw_hQdqJixrXlKzpVzdvHf2DqrPvdz7Y")
                .unwrap(),
            namespace: String::from("governance"),
            schema_id: String::from("governance"),
            owner: KeyIdentifier::from_str("ED8MpwKh3OjPEw_hQdqJixrXlKzpVzdvHf2DqrPvdz7Y").unwrap(),
            properties: String::from("governance"),
            keys: None,
            creator: KeyIdentifier::from_str("ED8MpwKh3OjPEw_hQdqJixrXlKzpVzdvHf2DqrPvdz7Y")
                .unwrap(),
        };
        db.set_notary_register(
            &KeyIdentifier::from_str("ED8MpwKh3OjPEw_hQdqJixrXlKzpVzdvHf2DqrPvdz7Y").unwrap(),
            &DigestIdentifier::from_str("Jg2Nuv5bNs4swQGcPQ1CXs9MtcfwMVoeQDR2Ea1YNYJw").unwrap(),
            DigestIdentifier::from_str("Jg2Nuv5bNs4swQGcPQ1CXs9MtcfwMVoeQDR2Ea1YNYJw").unwrap(),
            3,
        )
        .unwrap();
        db.set_subject(
            &DigestIdentifier::from_str("JKXo-EvPxQcL_nhbd4iprzyjdNxT9YYrmeJ7p5N_IVrg").unwrap(),
            subject,
        )
        .unwrap();
        // Shutdown channel
        let (bsx, _brx) = tokio::sync::broadcast::channel::<()>(10);
        let (a, b) = MpscChannel::<GovernanceMessage, GovernanceResponse>::new(100);
        let (c, d) = MpscChannel::<GovernanceUpdatedMessage, ()>::new(100);
        let (e, f) = MpscChannel::<GovernanceUpdatedMessage, ()>::new(100);
        let (_msg_channel_m, msg_channel_r) = MpscChannel::<TapleMessages, ()>::new(100);
        let gov_manager = Governance::new(a, bsx, _brx, db, f);
        let db = DB::new(manager);
        let notary = Notary::new(
            GovernanceAPI::new(b),
            db,
            SelfSignatureManager {
                keys: generate::<Ed25519KeyPair>(None),
                identifier: KeyIdentifier::from_str("ED8MpwKh3OjPEw_hQdqJixrXlKzpVzdvHf2DqrPvdz7Y")
                    .unwrap(),
                digest_derivator: DigestDerivator::Blake3_256,
            },
            msg_channel_r,
        );
        (gov_manager, notary)
    }

    fn not_ev(sn: u64) -> NotaryEvent {
        NotaryEvent {
            proof: ValidationProof {
                governance_id: DigestIdentifier::from_str(
                    "JKXo-EvPxQcL_nhbd4iprzyjdNxT9YYrmeJ7p5N_IVrg",
                )
                .unwrap(),
                subject_id: DigestIdentifier::from_str(
                    "JKXo-EvPxQcL_nhbd4iprzyjdNxT9YYrmeJ7p5N_IVrg",
                )
                .unwrap(),
                governance_version: 0,
                sn,
                schema_id: String::from("governance"),
                namespace: String::from("governance"),
                prev_event_hash: DigestIdentifier::from_str(
                    "", // Vacio porque el anterior es de génesis
                )
                .unwrap(),
                event_hash: DigestIdentifier::from_str(
                    "JKXo-EvPxQcL_nhbd4iprzyjdNxT9YYrmeJ7p5N_IVrg",
                )
                .unwrap(),
                state_hash: DigestIdentifier::from_str(
                    "governance",
                )
                .unwrap(),
                subject_public_key: KeyIdentifier::from_str("public_key")
                .unwrap(),
                owner: KeyIdentifier::from_str("ED8MpwKh3OjPEw_hQdqJixrXlKzpVzdvHf2DqrPvdz7Y")
                    .unwrap(),
                creator: KeyIdentifier::from_str("ED8MpwKh3OjPEw_hQdqJixrXlKzpVzdvHf2DqrPvdz7Y")
                    .unwrap(),
            },
            subject_signature: Signature {
                content: SignatureContent {
                    signer: KeyIdentifier::from_str("ED8MpwKh3OjPEw_hQdqJixrXlKzpVzdvHf2DqrPvdz7Y")
                    .unwrap(),
                    event_content_hash: DigestIdentifier::from_str(
                        "JKXo-EvPxQcL_nhbd4iprzyjdNxT9YYrmeJ7p5N_IVrg",
                    )
                    .unwrap(),
                    timestamp: TimeStamp::now(),
                },
                signature: SignatureIdentifier {
                    derivator: SignatureDerivator::Ed25519Sha512,
                    signature: vec![],
                },
            }
        }
    }
}
*/
