use std::collections::HashMap;

use crate::{
    commons::{bd::db::DB, errors::ChannelErrors},
    governance::{GovernanceAPI, GovernanceInterface},
    identifier::DigestIdentifier,
};

use super::{errors::NotaryError, NotaryEvent, NotaryEventResponse};

pub struct Notary {
    gov_api: GovernanceAPI,
    database: DB,
    cache_gov_ver: HashMap<DigestIdentifier, u32>,
}

impl Notary {
    pub fn new(gov_api: GovernanceAPI, database: DB) -> Self {
        Self {
            gov_api,
            database,
            cache_gov_ver: HashMap::new(),
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
        todo!()
    }
}
