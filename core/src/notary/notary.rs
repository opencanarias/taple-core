use std::collections::{HashMap, HashSet};

use crate::{
    commons::{
        channel::SenderEnd,
        errors::ChannelErrors,
        models::event::ValidationProof,
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
    ) -> Result<NotaryEventResponse, NotaryError> {
        let actual_gov_version = match self
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
                    vec![notary_event.proof.owner],
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
        if notary_event.subject_signature.verify().is_err()
            || proof_hash != notary_event.subject_signature.content.event_content_hash
        {
            return Err(NotaryError::SubjectSignatureNotValid);
        }
        match self
            .check_proofs(
                &notary_event.proof,
                notary_event.previous_proof,
                &notary_event.prev_event_validation_signatures,
                last_proof,
            )
            .await?
        {
            Some(subject_pk) => {
                if notary_event.subject_signature.content.signer != subject_pk {
                    return Err(NotaryError::SubjectSignatureNotValid);
                }
            }
            None => {}
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
            "SE ENVÍA LA VALIDACIÓN A {}",
            notary_event.proof.owner.to_str()
        );
        self.message_channel
            .tell(MessageTaskCommand::Request(
                None,
                TapleMessages::EventMessage(EventCommand::ValidatorResponse {
                    event_hash: notary_event.proof.event_hash,
                    signature: notary_signature.clone(),
                    governance_version: actual_gov_version,
                }),
                vec![notary_event.proof.owner],
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
        validation_signatures: &HashSet<Signature>,
        last_proof: Option<ValidationProof>,
    ) -> Result<Option<KeyIdentifier>, NotaryError> {
        if previous_proof.is_none() && new_proof.sn != 1 {
            return Err(NotaryError::PreviousProofLeft);
        }
        match last_proof {
            Some(last_proof) => {
                // Comprobar que tenemos la prueba del evento anterior, si no tenemos que hacer la comprobación de la que nos llega en el mensaje como cuando no tenemos el registro
                if last_proof.sn > new_proof.sn {
                    Err(NotaryError::EventSnLowerThanLastSigned)
                } else if last_proof.sn == new_proof.sn {
                    // Comprobar que solo cambia la versión de la governanza
                    if !last_proof.is_similar(&new_proof) {
                        Err(NotaryError::DifferentProofForEvent)
                    } else {
                        Ok(Some(last_proof.subject_public_key))
                    }
                } else if last_proof.sn + 1 == new_proof.sn {
                    // Comprobar que es similar a la prueba del evento anterior que nos llega en el mensaje
                    if !last_proof.is_similar(&previous_proof.unwrap())
                        || last_proof.event_hash != new_proof.prev_event_hash
                    {
                        Err(NotaryError::DifferentProofForEvent)
                    } else {
                        Ok(Some(last_proof.subject_public_key))
                    }
                } else {
                    // Mismo caso que en not found, no tengo la prueba anterior
                    if new_proof.sn == 1 {
                        // Nos tenemos que fiar:
                        // TODO: actualmente si el evento 0 no lo validamos no existe la prueba de sn 0, con lo que si el evento 1 es de transferencia ya se va a cambiar la public key del sujeto en la prueba (se pone la nueva), con lo que no puedo validar la firma del sujeto que viene en el evento 1 en este caso porque se va a firmar con otro material criptográfico (el que tenía previamente) entonces no me queda otra que fiarme de los eventos 1
                        Ok(None)
                    } else {
                        if previous_proof.is_none() {
                            return Err(NotaryError::PreviousProofLeft);
                        }
                        Ok(Some(
                            self.validate_previous_proof(
                                new_proof,
                                previous_proof.unwrap(),
                                validation_signatures,
                            )
                            .await?,
                        ))
                    }
                }
            }
            None => {
                // Comprobar la prueba de validación anterior junto con las firmas de validación de dicha prueba, su validez criptográfica y si llega a quorum
                if previous_proof.is_none() {
                    return Err(NotaryError::PreviousProofLeft);
                }
                Ok(Some(
                    self.validate_previous_proof(
                        new_proof,
                        previous_proof.unwrap(),
                        validation_signatures,
                    )
                    .await?,
                ))
            }
        }
    }

    async fn validate_previous_proof(
        &self,
        new_proof: &ValidationProof,
        previous_proof: ValidationProof,
        validation_signatures: &HashSet<Signature>,
    ) -> Result<KeyIdentifier, NotaryError> {
        // Comprobar que la previous encaja con la nueva
        if previous_proof.event_hash != new_proof.prev_event_hash {
            return Err(NotaryError::DifferentProofForEvent);
        }
        if previous_proof.sn + 1 != new_proof.sn {
            return Err(NotaryError::DifferentProofForEvent);
        }
        for signature in validation_signatures.iter() {
            signature
                .verify()
                .map_err(|_| NotaryError::InvalidSignature)?;
        }
        let actual_signers: Result<HashSet<KeyIdentifier>, NotaryError> = validation_signatures
            .iter()
            .map(|signature| {
                if signature.verify().is_err() {
                    return Err(NotaryError::InvalidSignature);
                }
                Ok(signature.content.signer.clone())
            })
            .collect();
        let actual_signers = actual_signers?;
        let (signers, quorum_size) = self
            .get_signers_and_quorum(previous_proof.get_metadata(), ValidationStage::Validate)
            .await?;
        if !actual_signers.is_subset(&signers) {
            return Err(NotaryError::InvalidSigner);
        }
        if actual_signers.len() < quorum_size as usize {
            return Err(NotaryError::QuorumNotReached);
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