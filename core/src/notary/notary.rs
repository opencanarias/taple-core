use std::collections::HashMap;

use crate::{
    commons::{
        bd::{db::DB, TapleDB},
        errors::ChannelErrors,
    },
    governance::{GovernanceAPI, GovernanceInterface},
    identifier::DigestIdentifier,
    protocol::command_head_manager::self_signature_manager::{
        SelfSignatureInterface, SelfSignatureManager,
    },
};

use super::{errors::NotaryError, NotaryEvent, NotaryEventResponse};

pub struct Notary {
    gov_api: GovernanceAPI,
    database: DB,
    cache_gov_ver: HashMap<DigestIdentifier, u32>,
    signature_manager: SelfSignatureManager,
}

impl Notary {
    pub fn new(
        gov_api: GovernanceAPI,
        database: DB,
        signature_manager: SelfSignatureManager,
    ) -> Self {
        Self {
            gov_api,
            database,
            cache_gov_ver: HashMap::new(),
            signature_manager,
        }
    }

    pub async fn notary_event(
        &mut self,
        notary_event: NotaryEvent,
    ) -> Result<NotaryEventResponse, NotaryError> {
        let actual_gov_version = match self
            .gov_api
            .get_governance_version(&notary_event.gov_id)
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
        let notary_register = match self
            .database
            .get_notary_register(&notary_event.owner, &notary_event.subject_id)
        {
            Some(nr) => nr,
            None => return Err(NotaryError::OwnerSubjectNotKnown),
        };
        if notary_register.1 > notary_event.sn {
            Err(NotaryError::EventSnLowerThanLastSigned)
        } else if notary_register.1 == notary_event.sn
            && notary_event.event_hash != notary_register.0
        {
            Err(NotaryError::DifferentHashForEvent)
        } else {
            // Get in DB, it is important that this goes first to ensure that we dont sign 2 different event_hash for the same event sn and subject
            self.database.set_notary_register(
                &notary_event.owner,
                &notary_event.subject_id,
                notary_event.event_hash.clone(),
                notary_event.sn,
            );
            // Now we sign and send
            let hash = DigestIdentifier::from_serializable_borsh((
                notary_event.gov_id,
                notary_event.subject_id,
                notary_event.owner,
                notary_event.event_hash,
                notary_event.sn,
                notary_event.gov_version,
            ))
            .map_err(|_| NotaryError::SerializingError)?;
            let notary_signature = self
                .signature_manager
                .sign(&(hash, actual_gov_version))
                .map_err(NotaryError::ProtocolErrors)?;
            Ok(NotaryEventResponse {
                notary_signature,
                gov_version_notary: actual_gov_version,
            })
        }
    }
}
