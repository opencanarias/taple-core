use std::collections::{HashSet, HashMap};

use crate::{governance::GovernanceInterface, protocol::request_manager::VotationType, identifier::DigestIdentifier};

use super::{ApprovalMessages, error::ApprovalManagerError, RequestApproval};

pub struct InnerApprovalManager<G: GovernanceInterface> {
    governance: G,
    // Cola de 1 elemento por sujeto
    subject_been_approved: HashSet<DigestIdentifier>,
    request_table: HashMap<DigestIdentifier, (DigestIdentifier, u64)>, // RequestID -> (SubjectID, GovVersion) Quizás meter SN
    pass_votation: VotationType,
}

impl<G: GovernanceInterface> InnerApprovalManager<G> {
    pub async fn process_approval_request(
        &mut self,
        approval_request: RequestApproval,
    ) -> Result<ApprovalMessages, ApprovalManagerError> {

        /*
          No es necesario que tengamos el sujeto.
          Comprobamos la versión de la gobernanza
            - Rechazamos las peticiones que tengan una versión de gobernanza distinta a la nuestra
          Comprobamos la validez criptográfica de la información que nos entrega.
            - Comprobar la firma de invocación.
            - Comprobar validez del invocador.
            - Comprobar las firmas de evaluación.
            - Comprobar la validez de los evaluadores.
         */

        // It is checked if the subject is possessed.
        // The cryptographic validity of the request is checked.
        // We check if we are synchronized.
        //   - It involves consulting the subject.
        //   - We will be able to process the request as long as the SN is equal to or less than the current one.
        // The request is not signed on the fly, but stored in memory. It does not need to be stored in DB.
        // A notification is sent to the user.

        // TODO: Attempt to ascertain the identity of the sender. One could sign with the subject to ensure that the message
        // comes only from the controller. This would allow us to keep a single vote in the DB. It could also be possible
        // with timestamp.

        let id = &approval_request.request.signature.content.event_content_hash;

        let id = approval_request
            .request
            .signature
            .content
            .event_content_hash
            .clone();
        approval_request
            .request
            .check_signatures()
            .map_err(|_| RequestManagerError::SignVerificationFailed)?;
        let EventRequestType::State(data) = approval_request.request.request.clone() else {
        return Ok((RequestManagerResponse::ApprovalRequest(Err(ResponseError::RequestTypeError)), None));
    };
        let None = self.request_stack.get(&data.subject_id) else {
        // TODO: It is possible that the vote has already been generated and we can respond.
        return Ok((RequestManagerResponse::ApprovalRequest(Err(ResponseError::RequestAlreadyKnown)), None));
    };
        let subject = match self.db.get_subject(&data.subject_id) {
            Ok(subject) => subject,
            Err(DbError::EntryNotFound) => {
                return Ok((
                    RequestManagerResponse::ApprovalRequest(Err(ResponseError::SubjectNotFound)),
                    None,
                ))
            }
            Err(error) => return Err(RequestManagerError::DatabaseError(error.to_string())),
        };
        let Some(subject_data) = subject.subject_data else {
        return Ok((RequestManagerResponse::ApprovalRequest(Err(ResponseError::SubjectNotFound)), None));
    };
        let invokation_permissions = self
            .governance_api
            .check_invokation_permission(
                data.subject_id.clone(),
                approval_request.request.signature.content.signer.clone(),
                None,
                None,
            )
            .await
            .map_err(|e| RequestManagerError::RequestError(e))?;
        if !invokation_permissions.0 {
            return Ok((
                RequestManagerResponse::ApprovalRequest(Err(ResponseError::InvalidCaller)),
                None,
            ));
        }
        if invokation_permissions.1 {
            if approval_request.expected_sn == subject_data.sn + 1 {
                // TODO: Revise according to phase 2
                self.request_table
                    .insert(id.clone(), (subject_data.subject_id.clone(), false));
                self.notifier
                    .request_reached(&id.to_str(), &subject_data.subject_id.to_str());
                self.to_approval_request.insert(
                    subject_data.subject_id,
                    (approval_request.request, approval_request.expected_sn),
                );
            } else if approval_request.expected_sn > subject_data.sn + 1 {
                return Ok((
                    RequestManagerResponse::ApprovalRequest(Err(
                        ResponseError::NoSynchronizedSubject,
                    )),
                    None,
                ));
            } else {
                return Ok((
                    RequestManagerResponse::ApprovalRequest(Err(
                        ResponseError::EventAlreadyOnChain,
                    )),
                    None,
                ));
            }
        } else {
            return Ok((
                RequestManagerResponse::ApprovalRequest(Err(ResponseError::ApprovalNotNeeded)),
                None,
            ));
        }
        match self.pass_votation {
            VotationType::Normal => {
                // TODO: Pending the management of the data structure.
                Ok((RequestManagerResponse::ApprovalRequest(Ok(())), None))
            }
            VotationType::AlwaysAccept => {
                self.process_approval_resolve(&id, Acceptance::Accept).await
            }
            VotationType::AlwaysReject => {
                self.process_approval_resolve(&id, Acceptance::Reject).await
            }
        }
    }
}
