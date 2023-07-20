use crate::{
    commons::{
        config::VotationType,
        models::{
            approval::{ApprovalEntity, ApprovalResponse, ApprovalState},
            event::Metadata,
            state::{generate_subject_id, Subject},
        },
        self_signature_manager::{SelfSignatureInterface, SelfSignatureManager},
    },
    database::DB,
    governance::{error::RequestError, GovernanceInterface},
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
    request::EventRequest,
    signature::Signed,
    ApprovalRequest, DatabaseCollection, Notification,
};

use super::error::{ApprovalErrorResponse, ApprovalManagerError};

pub trait NotifierInterface {
    fn request_reached(&self, id: &str, subject_id: &str, sn: u64);
    fn request_obsolete(&self, id: String, subject_id: String, sn: u64);
}

pub struct RequestNotifier {
    sender: tokio::sync::broadcast::Sender<Notification>,
}

impl RequestNotifier {
    pub fn new(sender: tokio::sync::broadcast::Sender<Notification>) -> Self {
        Self { sender }
    }
}

impl NotifierInterface for RequestNotifier {
    fn request_reached(&self, id: &str, subject_id: &str, sn: u64) {
        let _ = self.sender.send(Notification::ApprovalReceived {
            id: id.to_owned(),
            subject_id: subject_id.to_owned(),
            sn,
        });
    }

    fn request_obsolete(&self, id: String, subject_id: String, sn: u64) {
        let _ = self.sender.send(Notification::ObsoletedApproval {
            id: id,
            subject_id: subject_id,
            sn,
        });
    }
}

pub struct InnerApprovalManager<G: GovernanceInterface, N: NotifierInterface, C: DatabaseCollection>
{
    governance: G,
    database: DB<C>,
    notifier: N,
    signature_manager: SelfSignatureManager,
    // Cola de 1 elemento por sujeto
    // subject_been_approved: HashMap<DigestIdentifier, DigestIdentifier>, // SubjectID -> ReqId
    pass_votation: VotationType,
}

impl<G: GovernanceInterface, N: NotifierInterface, C: DatabaseCollection>
    InnerApprovalManager<G, N, C>
{
    pub fn new(
        governance: G,
        database: DB<C>,
        notifier: N,
        signature_manager: SelfSignatureManager,
        pass_votation: VotationType,
    ) -> Self {
        Self {
            governance,
            database,
            notifier,
            signature_manager,
            // subject_been_approved: HashMap::new(),
            pass_votation,
        }
    }

    pub fn get_single_request(
        &self,
        request_id: &DigestIdentifier,
    ) -> Result<ApprovalEntity, ApprovalErrorResponse> {
        let request = self
            .database
            .get_approval(request_id)
            .map_err(|_| ApprovalErrorResponse::ApprovalRequestNotFound)?;
        Ok(request)
    }

    pub fn get_all_request(&self) -> Vec<ApprovalEntity> {
        self.database
            .get_approvals(Some(ApprovalState::Pending), None, isize::MAX)
            .unwrap()
    }

    #[allow(dead_code)]
    pub fn change_pass_votation(&mut self, pass_votation: VotationType) {
        self.pass_votation = pass_votation;
    }

    pub async fn get_governance_version(
        &self,
        governance_id: &DigestIdentifier,
        subject_id: &DigestIdentifier,
    ) -> Result<Result<u64, ApprovalErrorResponse>, ApprovalManagerError> {
        match self
            .governance
            .get_governance_version(governance_id.to_owned(), subject_id.clone())
            .await
        {
            Ok(data) => Ok(Ok(data)),
            Err(RequestError::GovernanceNotFound(_str)) => {
                Ok(Err(ApprovalErrorResponse::GovernanceNotFound))
            }
            Err(RequestError::InvalidGovernanceID) => {
                Ok(Err(ApprovalErrorResponse::InvalidGovernanceID))
            }
            Err(RequestError::ChannelClosed) => Err(ApprovalManagerError::GovernanceChannelFailed),
            Err(_error) => Err(ApprovalManagerError::UnexpectedError),
        }
    }

    pub fn new_governance_version(
        &mut self,
        governance_id: &DigestIdentifier,
    ) -> Result<(), ApprovalManagerError> {
        // Comprobamos todas las peticiones guardadas y borramos las afectadas
        let affected_requests = self
            .database
            .get_approvals_by_governance(governance_id)
            .map_err(|_| ApprovalManagerError::DatabaseError)?;
        for request in affected_requests {
            // Borrarlas de la colección principal y del índice
            let approval_entity = self
                .database
                .get_approval(&request)
                .map_err(|_| ApprovalManagerError::DatabaseError)?;
            let subject_id = {
                match approval_entity.request.content.event_request.content {
                    EventRequest::Fact(ref fact_request) => fact_request.subject_id.clone(),
                    EventRequest::Create(ref create_request) => generate_subject_id(
                        &create_request.namespace,
                        &create_request.schema_id,
                        create_request.public_key.to_str(),
                        create_request.governance_id.to_str(),
                        approval_entity.request.content.gov_version,
                    )
                    .map_err(|_| ApprovalManagerError::UnexpectedError)?,
                    _ => return Err(ApprovalManagerError::UnexpectedRequestType),
                }
            };
            self.notifier.request_obsolete(
                approval_entity.id.to_str(),
                subject_id.to_str(),
                approval_entity.request.content.sn,
            );
            self.database
                .del_approval(&request)
                .map_err(|_| ApprovalManagerError::DatabaseError)?;
            self.database
                .del_governance_aproval_index(&governance_id, &request)
                .map_err(|_| ApprovalManagerError::DatabaseError)?;
            self.database
                .del_subject_aproval_index(&subject_id, &request)
                .map_err(|_| ApprovalManagerError::DatabaseError)?;
        }
        Ok(())
    }

    pub async fn process_approval_request(
        &mut self,
        approval_request: Signed<ApprovalRequest>,
        sender: KeyIdentifier,
    ) -> Result<
        Result<Option<(Signed<ApprovalResponse>, KeyIdentifier)>, ApprovalErrorResponse>,
        ApprovalManagerError,
    > {
        /*
            THE APPROVER IS NOW ALSO A WITNESS
            - Check if the subject is possessed
            - Check if we are synchronized
            We check the governance version
                - We reject requests that have a governance version different from ours
            We check the cryptographic validity of the information given to us.
                - Check the invocation signature.
                - Check the validity of the invoker.
                - Check the evaluation signatures.
                - Check the validity of the evaluators.
            The requests will not be saved in the DB, but in memory.
            Only one request per subject will be saved. There is the problem that an event has been approved without our intervention.
            intervention. In this case it is necessary to delete the request and update to the new one.
            We must always check if we already have the request sent to us.
        */
        let id: DigestIdentifier =
            match DigestIdentifier::from_serializable_borsh(&approval_request.content)
                .map_err(|_| ApprovalErrorResponse::ErrorHashing)
            {
                Ok(id) => id,
                Err(error) => return Ok(Err(error)),
            };

        if let Ok(data) = self.get_single_request(&id) {
            match data.state {
                ApprovalState::Pending | ApprovalState::Obsolete => {
                    return Ok(Err(ApprovalErrorResponse::RequestAlreadyKnown))
                }
                ApprovalState::RespondedAccepted | ApprovalState::RespondedRejected => {
                    let result = self
                        .generate_vote(&id, data.response.expect("Should be").content.approved)
                        .await?;
                    let (vote, sender) = result.expect("Request should be in data structure");
                    return Ok(Ok(Some((vote.response.unwrap(), sender))));
                }
            }
        };

        // We check if we are already approving the subject for an equal or greater event.
        // If there is no previous request, we continue.
        let subject_id = subject_id_by_request(
            &approval_request.content.event_request.content,
            approval_request.content.gov_version,
        )?;
        let request_queue = self
            .database
            .get_approvals_by_subject(&subject_id)
            .map_err(|_| ApprovalManagerError::DatabaseError)?;
        if request_queue.len() == 1 {
            let data = self.get_single_request(&request_queue[0]).unwrap();
            if approval_request.content.sn <= data.request.content.sn {
                return Ok(Err(ApprovalErrorResponse::PreviousEventDetected));
            }
        } else if request_queue.len() != 0 {
            return Err(ApprovalManagerError::MoreRequestThanMaxAllowed);
        }

        // We check if the governance version is correct
        let version = match self
            .get_governance_version(&approval_request.content.gov_id, &subject_id)
            .await?
        {
            Ok(version) => version,
            Err(error) => return Ok(Err(error)),
        };

        let request_gov_version = approval_request.content.gov_version;

        if version > request_gov_version {
            // Nuestra gov es mayor: mandamos mensaje para que actualice el emisor
            return Ok(Err(ApprovalErrorResponse::OurGovIsHigher {
                our_id: self.signature_manager.get_own_identifier(),
                sender,
                gov_id: approval_request.content.gov_id.clone(),
            }));
        } else if version < request_gov_version {
            // Nuestra gov es menor: no podemos hacer nada. Pedimos LCE al que nos lo envió
            return Ok(Err(ApprovalErrorResponse::OurGovIsLower {
                our_id: self.signature_manager.get_own_identifier(),
                sender,
                gov_id: approval_request.content.gov_id.clone(),
            }));
        }

        // The EventRequest is correct. We can move on to save it in the system if applicable.
        // This will depend on the Flag PassVotation
        // - VotationType::Normal => It is saved in the system waiting for the user.
        // - VotarionType::AlwaysAccept => Yes vote is cast
        // - VotarionType::AlwaysReject => Negative vote cast
        let gov_id = approval_request.content.gov_id.clone();
        let sn = approval_request.content.sn;
        let approval_entity = ApprovalEntity {
            id: id.clone(),
            request: approval_request,
            response: None,
            state: ApprovalState::Pending,
            sender,
        };
        self.database
            .set_subject_aproval_index(&subject_id, &id)
            .map_err(|_| ApprovalManagerError::DatabaseError)?;
        if !gov_id.digest.is_empty() {
            self.database
                .set_governance_aproval_index(&gov_id, &id)
                .map_err(|_| ApprovalManagerError::DatabaseError)?;
        }
        let Ok(_result) = self.database.set_approval(&id, approval_entity) else {
            return Err(ApprovalManagerError::DatabaseError);
        };
        self.notifier
            .request_reached(&id.to_str(), &subject_id.to_str(), sn);

        match self.pass_votation {
            VotationType::Normal => return Ok(Ok(None)),
            VotationType::AlwaysAccept => {
                let (vote, sender) = self
                    .generate_vote(&id, true)
                    .await?
                    .expect("Request should be in data structure");
                return Ok(Ok(Some((vote.response.unwrap(), sender))));
            }
            VotationType::AlwaysReject => {
                let (vote, sender) = self
                    .generate_vote(&id, false)
                    .await?
                    .expect("Request should be in data structure");
                return Ok(Ok(Some((vote.response.unwrap(), sender))));
            }
        }
    }

    pub async fn generate_vote(
        &mut self,
        request_id: &DigestIdentifier,
        acceptance: bool,
    ) -> Result<Result<(ApprovalEntity, KeyIdentifier), ApprovalErrorResponse>, ApprovalManagerError>
    {
        // Obtenemos la petición
        let Ok(mut data) = self.get_single_request(&request_id) else {
            return Ok(Err(ApprovalErrorResponse::RequestNotFound));
        };
        if let ApprovalState::RespondedAccepted = data.state {
            return Ok(Err(ApprovalErrorResponse::RequestAlreadyResponded));
        } else if ApprovalState::RespondedRejected == data.state {
            return Ok(Err(ApprovalErrorResponse::RequestAlreadyResponded));
        }
        let response = ApprovalResponse {
            appr_req_hash: request_id.clone(),
            approved: acceptance,
        };
        let subject_id = subject_id_by_request(
            &data.request.content.event_request.content,
            data.request.content.gov_version,
        )?;
        let signature = self
            .signature_manager
            .sign(&response)
            .map_err(|_| ApprovalManagerError::SignProcessFailed)?;
        data.state = if acceptance {
            ApprovalState::RespondedAccepted
        } else {
            ApprovalState::RespondedRejected
        };
        data.response = Some(Signed::<ApprovalResponse> {
            content: response,
            signature,
        });
        let Ok(_result) = self.database.set_approval(&request_id, data.clone()) else {
            return Err(ApprovalManagerError::DatabaseError)
        };
        self.database
            .del_subject_aproval_index(&subject_id, request_id)
            .map_err(|_| ApprovalManagerError::DatabaseError)?;
        self.database
            .del_governance_aproval_index(&data.request.content.gov_id, request_id)
            .map_err(|_| ApprovalManagerError::DatabaseError)?;
        let sender = data.sender.clone();
        Ok(Ok((data, sender)))
    }
}

#[allow(dead_code)]
fn event_proposal_hash_gen(
    approval_request: &Signed<ApprovalRequest>,
) -> Result<DigestIdentifier, ApprovalManagerError> {
    Ok(DigestIdentifier::from_serializable_borsh(approval_request)
        .map_err(|_| ApprovalManagerError::HashGenerationFailed)?)
}

#[allow(dead_code)]
fn create_metadata(subject_data: &Subject, governance_version: u64) -> Metadata {
    Metadata {
        namespace: subject_data.namespace.clone(),
        subject_id: subject_data.subject_id.clone(),
        governance_id: subject_data.governance_id.clone(),
        governance_version,
        schema_id: subject_data.schema_id.clone(),
    }
}

fn subject_id_by_request(
    request: &EventRequest,
    gov_version: u64,
) -> Result<DigestIdentifier, ApprovalManagerError> {
    let subject_id = match request {
        EventRequest::Fact(ref fact_request) => fact_request.subject_id.clone(),
        EventRequest::Create(ref create_request) => generate_subject_id(
            &create_request.namespace,
            &create_request.schema_id,
            create_request.public_key.to_str(),
            create_request.governance_id.to_str(),
            gov_version,
        )
        .map_err(|_| ApprovalManagerError::UnexpectedError)?,
        _ => return Err(ApprovalManagerError::UnexpectedRequestType),
    };
    Ok(subject_id)
}
