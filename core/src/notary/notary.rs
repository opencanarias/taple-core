use std::collections::HashMap;

use crate::{
    commons::{
        channel::SenderEnd,
        errors::ChannelErrors,
        self_signature_manager::{SelfSignatureInterface, SelfSignatureManager},
    },
    event::EventCommand,
    governance::{GovernanceAPI, GovernanceInterface},
    identifier::DigestIdentifier,
    message::{MessageConfig, MessageTaskCommand},
    protocol::protocol_message_manager::TapleMessages,
};

use super::{errors::NotaryError, NotaryEvent, NotaryEventResponse};
use crate::database::{DatabaseManager, DB};

pub struct Notary<D: DatabaseManager> {
    gov_api: GovernanceAPI,
    database: DB<D>,
    cache_gov_ver: HashMap<DigestIdentifier, u32>,
    signature_manager: SelfSignatureManager,
    message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
}

impl<D: DatabaseManager> Notary<D> {
    pub fn new(
        gov_api: GovernanceAPI,
        database: DB<D>,
        signature_manager: SelfSignatureManager,
        message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    ) -> Self {
        Self {
            gov_api,
            database,
            cache_gov_ver: HashMap::new(),
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
            .get_governance_version(notary_event.gov_id.clone(), notary_event.subject_id.clone())
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
        if actual_gov_version < notary_event.gov_version {
            return Err(NotaryError::GovernanceVersionTooHigh);
        }
        match self
            .database
            .get_notary_register(&notary_event.owner, &notary_event.subject_id)
        {
            Ok(notary_register) => {
                if notary_register.1 > notary_event.sn {
                    return Err(NotaryError::EventSnLowerThanLastSigned);
                } else if notary_register.1 == notary_event.sn
                    && notary_event.event_hash != notary_register.0
                {
                    return Err(NotaryError::DifferentHashForEvent);
                }
            }
            Err(error) => match error {
                crate::DbError::EntryNotFound => {}
                _ => return Err(NotaryError::DatabaseError),
            },
        };
        // Get in DB, it is important that this goes first to ensure that we dont sign 2 different event_hash for the same event sn and subject
        self.database
            .set_notary_register(
                &notary_event.owner,
                &notary_event.subject_id,
                notary_event.event_hash.clone(),
                notary_event.sn,
            )
            .map_err(|_| NotaryError::DatabaseError)?;
        // Now we sign and send
        // let hash = DigestIdentifier::from_serializable_borsh((
        //     &notary_event.gov_id,
        //     &notary_event.subject_id,
        //     &notary_event.owner,
        //     &notary_event.event_hash,
        //     &notary_event.sn,
        //     &notary_event.gov_version,
        // ))
        // .map_err(|_| NotaryError::SerializingError)?;
        let notary_signature = self
            .signature_manager
            .sign(&(
                &notary_event.gov_id,
                &notary_event.subject_id,
                &notary_event.owner,
                &notary_event.event_hash,
                &notary_event.sn,
                &notary_event.gov_version,
            ))
            .map_err(NotaryError::ProtocolErrors)?;
        self.message_channel
            .tell(MessageTaskCommand::Request(
                None,
                TapleMessages::EventMessage(EventCommand::ValidatorResponse {
                    event_hash: notary_event.event_hash.clone(),
                    signature: notary_signature.clone(),
                }),
                vec![notary_event.owner],
                MessageConfig::direct_response(),
            ))
            .await?; // TODO borrar clone
        Ok(NotaryEventResponse {
            notary_signature,
            gov_version_notary: actual_gov_version,
        })
    }
}

/*
#[cfg(test)]
mod tests {
    use crate::commons::self_signature_manager::SelfSignatureManager;
    use crate::database::{MemoryManager, DB};
    use crate::governance::GovernanceUpdatedMessage;
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

    fn initialize() -> (Governance<MemoryManager>, Notary<MemoryManager>) {
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
        );
        (gov_manager, notary)
    }

    fn not_ev(sn: u64) -> NotaryEvent {
        NotaryEvent {
            gov_id: DigestIdentifier::from_str(
                "JKXo-EvPxQcL_nhbd4iprzyjdNxT9YYrmeJ7p5N_IVrg",
            )
            .unwrap(),
            subject_id: DigestIdentifier::from_str(
                "JKXo-EvPxQcL_nhbd4iprzyjdNxT9YYrmeJ7p5N_IVrg",
            )
            .unwrap(),
            owner: KeyIdentifier::from_str("ED8MpwKh3OjPEw_hQdqJixrXlKzpVzdvHf2DqrPvdz7Y")
            .unwrap(),
            event_hash: DigestIdentifier::from_str(
                "JKXo-EvPxQcL_nhbd4iprzyjdNxT9YYrmeJ7p5N_IVrg",
            )
            .unwrap(),
            sn,
            gov_version: 0,
            owner_signature: Signature { content: SignatureContent {
                signer: KeyIdentifier::from_str("ED8MpwKh3OjPEw_hQdqJixrXlKzpVzdvHf2DqrPvdz7Y")
                .unwrap(),
                event_content_hash: DigestIdentifier::from_str(
                    "JKXo-EvPxQcL_nhbd4iprzyjdNxT9YYrmeJ7p5N_IVrg",
                )
                .unwrap(),
                timestamp: TimeStamp::now(),
            }, signature: SignatureIdentifier::from_str("SEB2W98DwIvqL4BPIRnHOpogfn1qkNrOoSI-KJxSaLOoudEFo_Q6-FlMJvwBDQY3iGQ7iB4bcwr8QBgP8he7HVDA").unwrap() },
        }
    }
}
 */
