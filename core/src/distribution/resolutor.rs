use std::collections::{BTreeSet, HashMap, HashSet};

use crate::database::Error as DbError;
use crate::governance::error::RequestError;
use crate::governance::RequestQuorum;
use crate::{
    database::DB, governance::GovernanceInterface, identifier::DigestIdentifier, DatabaseManager,
    Event, SubjectData,
};

use super::error::{DistributionErrorResponses, DistributionManagerError};
use super::SetEventMessage;

#[derive(Ord, PartialOrd, Eq, PartialEq)]
enum CheckCommands {
    EventLink,
    CheckApprovals,
    CheckNotariesSignatures,
    CheckSubjectIsSigner,
    CheckSignatures,
    NotariesSignaturesExistence,
    SynchronizationEventNeeded,
    CheckEvaluationSignatures,
}

pub struct DistributionChecksResolutor<'a, D: DatabaseManager, G: GovernanceInterface> {
    synchronization_map: &'a HashMap<DigestIdentifier, (u64, DigestIdentifier, u64)>,
    db: &'a DB<D>,
    governance: &'a G,
    subject_data: Option<&'a SubjectData>,
    commands: BTreeSet<CheckCommands>,
}

impl<'a, D: DatabaseManager, G: GovernanceInterface> DistributionChecksResolutor<'a, D, G> {
    pub fn new(
        synchronization_map: &'a HashMap<DigestIdentifier, (u64, DigestIdentifier, u64)>,
        db: &'a DB<D>,
        governance: &'a G,
    ) -> Self {
        Self {
            synchronization_map,
            db,
            governance,
            subject_data: None,
            commands: BTreeSet::new(),
        }
    }

    pub fn with_subject_data(mut self, subject_data: &'a SubjectData) -> Self {
        self.subject_data = Some(subject_data);
        self
    }

    pub fn check_synchronization_event_needed(mut self) -> Self {
        self.commands
            .insert(CheckCommands::SynchronizationEventNeeded);
        self
    }

    pub fn check_evaluator_signatures(mut self) -> Self {
        self.commands
            .insert(CheckCommands::CheckEvaluationSignatures);
        self
    }

    pub fn check_notaries_signatures_existence(mut self) -> Self {
        self.commands
            .insert(CheckCommands::NotariesSignaturesExistence);
        self
    }

    pub fn check_signatures(mut self) -> Self {
        self.commands.insert(CheckCommands::CheckSignatures);
        self
    }

    pub fn check_subject_is_signer(mut self) -> Self {
        self.commands.insert(CheckCommands::CheckSubjectIsSigner);
        self
    }

    pub fn check_notaries_signatures(mut self) -> Self {
        self.commands.insert(CheckCommands::CheckNotariesSignatures);
        self
    }

    pub fn check_event_link(mut self) -> Self {
        self.commands.insert(CheckCommands::EventLink);
        self
    }

    pub fn check_approvals(mut self) -> Self {
        self.commands.insert(CheckCommands::CheckApprovals);
        self
    }

    fn event_link_exec(
        &self,
        event: &Event,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        // Comprobamos enlace a través del hash
        let subject_data = self.subject_data.as_ref().unwrap();
        match self.db.get_event(&subject_data.subject_id, subject_data.sn) {
            Ok(head_event) => {
                if head_event.signature.content.event_content_hash
                    != event.event_content.previous_hash
                {
                    return Ok(Err(DistributionErrorResponses::InvalidEventLink));
                }
            }
            Err(DbError::EntryNotFound) => {
                return Err(DistributionManagerError::DatabaseMismatch);
            }
            Err(error) => {
                return Err(DistributionManagerError::DatabaseError(error.to_string()));
            }
        }
        Ok(Ok(()))
    }

    fn check_signatures_exec(
        &self,
        event: &Event,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        // Comprobamos la validez de las firmas del evento + event_request
        if let Err(error) = event.check_signatures() {
            return Ok(Err(DistributionErrorResponses::InvalidEventSignatures));
        }
        Ok(Ok(()))
    }

    fn check_subject_is_signer_exec(
        &self,
        event: &Event,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        // Comprobamos que el firmante del evento haya sido el sujeto
        let subject_data = self.subject_data.as_ref().unwrap();
        if event.signature.content.signer != subject_data.public_key {
            return Ok(Err(DistributionErrorResponses::InvalidSubjectSignature));
        }
        Ok(Ok(()))
    }

    async fn check_approvals_exec(
        &self,
        event: &Event,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        let (is_valid_invokator, approval_needed) = self
            .governance
            .check_invokation_permission(
                event.event_content.subject_id.clone(),
                event
                    .event_content
                    .event_request
                    .signature
                    .content
                    .signer
                    .clone(),
                None,
                Some(event.event_content.metadata.clone()), //TODO: Comprobar necesidad
            )
            .await
            .map_err(|_| DistributionManagerError::GovernanceChannelNotAvailable)?;
        if !is_valid_invokator {
            return Ok(Err(DistributionErrorResponses::InvalidInvokator));
        }
        if approval_needed {
            let approvers = match self
                .governance
                .get_approvers(event.event_content.event_request.clone())
                .await
            {
                Ok(approvers) => approvers,
                Err(RequestError::InvalidRequestType) => {
                    return Ok(Err(DistributionErrorResponses::InvalidRequestType));
                }
                Err(RequestError::GovernanceNotFound(id)) => {
                    return Ok(Err(DistributionErrorResponses::GovernanceNotFound(id)));
                }
                Err(RequestError::InvalidKeyIdentifier(id)) => {
                    return Ok(Err(DistributionErrorResponses::InvalidKeyIdentifier(id)));
                }
                Err(error) => {
                    return Err(DistributionManagerError::UnexpectedError);
                }
            };
            // TODO: No me convencen los parámetros del método de gobernanza. Analizar cambiarlo o
            // sacar las apobaciones de la Event Request
            match self
                .governance
                .check_quorum_request(
                    event.event_content.event_request.clone(),
                    event.event_content.event_request.approvals.clone(),
                )
                .await
            {
                Ok((quorum_state, _)) => {
                    // Comprobamos si el estadod el quorum coincide con el especificado
                    if quorum_state == RequestQuorum::Processing {
                        return Ok(Err(DistributionErrorResponses::ApprovalQuorumNotReached));
                    }
                    if (event.event_content.approved && quorum_state != RequestQuorum::Accepted)
                        || (!event.event_content.approved
                            && quorum_state != RequestQuorum::Rejected)
                    {
                        // La indicación de aprobación es errónea
                        return Ok(Err(DistributionErrorResponses::ApprovalQuorumMismatch));
                    }
                }
                Err(RequestError::InvalidKeyIdentifier(id)) => {
                    // Un aprobador no es válido
                    return Ok(Err(DistributionErrorResponses::InvalidKeyIdentifier(id)));
                }
                Err(_) => {
                    return Err(DistributionManagerError::UnexpectedError);
                }
            }
        }
        Ok(Ok(()))
    }

    async fn check_notaries_signatures_exec(
        &self,
        event: &Event,
        message: &SetEventMessage,
    ) -> Result<Result<DigestIdentifier, DistributionErrorResponses>, DistributionManagerError>
    {
        let signatures = if let Some(signatures) = message.notaries_signatures.as_ref() {
            signatures.clone()
        } else {
            HashSet::new()
        };
        let hash = DigestIdentifier::from_serializable_borsh((
            &event.event_content.metadata.governance_id,
            &event.event_content.subject_id,
            &event.event_content.metadata.owner,
            &event.event_content.state_hash, // TODO: Consultar si eventHash == stateHash
            &event.event_content.sn,
            &event.event_content.metadata.governance_version,
        ))
        .map_err(|_| DistributionManagerError::HashGenerationFailed)?;

        if let Err(error) = self
            .governance
            .check_notary_signatures(
                signatures,
                hash,
                event.event_content.metadata.governance_id.clone(),
                event.event_content.metadata.namespace.clone(),
            )
            .await
        {
            return Ok(Err(DistributionErrorResponses::InvalidNotarySignatures));
        };
        Ok(Ok(event.signature.content.event_content_hash.clone()))
    }

    fn notaries_signatures_existence_exec(
        &self,
        message: &SetEventMessage,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        if let None = &message.notaries_signatures {
            return Ok(Err(DistributionErrorResponses::NoNotariesSignature));
        }
        Ok(Ok(()))
    }

    fn check_synchronization_event_needed_exec(
        &self,
        event: &Event,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        if let Some((_, _, expected_sn)) = self.synchronization_map.get(&event.event_content.subject_id) {
            if *expected_sn != event.event_content.sn {
                return Ok(Err(DistributionErrorResponses::EventNotNeeded));
            }
        }
        Ok(Ok(()))
    }

    async fn check_evaluator_signatures_exec(
        &self,
        event: &Event,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        if let Err(_error) = self
            .governance
            .check_evaluator_signatures(
                HashSet::new(),
                event.event_content.metadata.governance_id.clone(),
                event.event_content.metadata.governance_version,
                event.event_content.metadata.namespace.clone(),
            )
            .await
        {
            return Ok(Err(DistributionErrorResponses::InvalidEvaluatorSignatures));
        }
        Ok(Ok(()))
    }

    pub async fn execute(
        self,
        event: &Event,
        message: &SetEventMessage,
    ) -> Result<
        Result<Option<DigestIdentifier>, DistributionErrorResponses>,
        DistributionManagerError,
    > {
        let mut result = None;
        for command in self.commands.iter() {
            match command {
                CheckCommands::EventLink => {
                    if let Err(error) = self.event_link_exec(event)? {
                        return Ok(Err(error));
                    }
                }
                CheckCommands::CheckApprovals => {
                    if let Err(error) = self.check_approvals_exec(event).await? {
                        return Ok(Err(error));
                    }
                }
                CheckCommands::CheckNotariesSignatures => {
                    match self.check_notaries_signatures_exec(event, message).await? {
                        Ok(hash) => result = Some(hash),
                        Err(error) => return Ok(Err(error)),
                    }
                }
                CheckCommands::CheckSubjectIsSigner => {
                    if let Err(error) = self.check_subject_is_signer_exec(event)? {
                        return Ok(Err(error));
                    }
                }
                CheckCommands::CheckSignatures => {
                    if let Err(error) = self.check_signatures_exec(event)? {
                        return Ok(Err(error));
                    }
                }
                CheckCommands::NotariesSignaturesExistence => {
                    if let Err(error) = self.notaries_signatures_existence_exec(message)? {
                        return Ok(Err(error));
                    }
                }
                CheckCommands::SynchronizationEventNeeded => {
                    if let Err(error) = self.check_synchronization_event_needed_exec(event)? {
                        return Ok(Err(error));
                    }
                }
                CheckCommands::CheckEvaluationSignatures => {
                    if let Err(error) = self.check_evaluator_signatures_exec(event).await? {
                        return Ok(Err(error));
                    }
                }
            }
        }
        Ok(Ok(result))
    }
}
