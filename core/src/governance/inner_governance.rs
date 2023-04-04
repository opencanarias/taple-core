use std::{collections::HashSet, str::FromStr};

use crate::{
    commons::{
        identifier::{Derivable, DigestIdentifier, KeyIdentifier},
        models::{
            approval_signature::ApprovalResponse,
            event::Event,
            event_content::Metadata,
            event_request::{EventRequest, EventRequestType},
        },
    },
    database::Error as DbError,
};
use serde_json::Value;

use super::{
    error::{InternalError, RequestError},
    RequestQuorum,
};
use crate::commons::models::event_request::EventRequestType::State;
use crate::commons::models::event_request::RequestPayload::Json;

use crate::database::{DatabaseManager, DB};

pub struct InnerGovernance<D: DatabaseManager> {
    repo_access: DB<D>,
    governance_schema: Value,
}

impl<D: DatabaseManager> InnerGovernance<D> {
    pub fn new(repo_access: DB<D>, governance_schema: Value) -> InnerGovernance<D> {
        Self {
            repo_access,
            governance_schema,
        }
    }

    pub fn get_schema(
        &self,
        governance_id: DigestIdentifier,
        schema_id: String,
    ) -> Result<Result<Value, RequestError>, InternalError> {
        if governance_id.digest.is_empty() {
            return Ok(Ok(self.governance_schema.clone()));
        }
        let governance = self.repo_access.get_subject(&governance_id);
        let governance = match governance {
            Ok(governance) => governance,
            Err(DbError::EntryNotFound) => {
                return Ok(Err(RequestError::GovernanceNotFound(
                    governance_id.to_str(),
                )))
            }
            Err(error) => return Err(InternalError::DatabaseError { source: error }),
        };
        let properties: Value = serde_json::from_str(&governance.subject_data.unwrap().properties)
            .map_err(|_| InternalError::DeserializationError)?;
        let schemas = get_as_array(&properties, "schemas")?;
        for schema in schemas {
            let tmp = get_as_str(schema, "id")?;
            if tmp == &schema_id {
                return Ok(Ok(schema.get("State-Schema").unwrap().to_owned()));
            }
        }
        return Ok(Err(RequestError::SchemaNotFound(schema_id)));
    }

    pub fn get_validators(
        &self,
        event: Event,
    ) -> Result<Result<HashSet<KeyIdentifier>, RequestError>, InternalError> {
        let governance_id = if event.event_content.metadata.governance_id.digest.is_empty() {
            event.event_content.subject_id.clone()
        } else {
            event.event_content.metadata.governance_id.clone()
        };
        let governance = self.repo_access.get_subject(&governance_id);
        let governance = match governance {
            Ok(governance) => governance,
            Err(DbError::EntryNotFound) => return Self::parche(event),
            Err(error) => return Err(InternalError::DatabaseError { source: error }),
        };
        if governance.subject_data.is_some() {
            let properties: Value =
                serde_json::from_str(&governance.subject_data.unwrap().properties)
                    .map_err(|_| InternalError::DeserializationError)?;
            let policies = get_as_array(&properties, "policies")?;
            let schema_policy =
                get_schema_from_policies(policies, &event.event_content.metadata.schema_id);
            let Ok(schema_policy) = schema_policy else {
                return Ok(Err(schema_policy.unwrap_err()));
            };
            let validators = get_as_array(&schema_policy.get("validation").unwrap(), "validators")?;
            let all_validators = get_members_from_set(&validators);
            let Ok(all_validators) = all_validators else {
                return Ok(Err(all_validators.unwrap_err()));
            };
            Ok(Ok(all_validators))
        } else {
            Self::parche(event)
        }
    }

    pub fn get_approvers(
        &self,
        event_request: EventRequest,
    ) -> Result<Result<HashSet<KeyIdentifier>, RequestError>, InternalError> {
        let EventRequestType::State(request) = event_request.request else {
            return Ok(Err(RequestError::InvalidRequestType))
        };
        let subject = match self.repo_access.get_subject(&request.subject_id) {
            Ok(subject) => subject,
            Err(DbError::EntryNotFound) => return Ok(Err(RequestError::SubjectNotFound)),
            Err(error) => return Err(InternalError::DatabaseError { source: error }),
        };
        let Some(subject_data) = subject.subject_data else {
            return Ok(Err(RequestError::SubjectNotFound))
        };
        let governance_id = subject_data.governance_id.clone();
        let governance = match self.repo_access.get_subject(&governance_id) {
            Ok(governance) => governance,
            Err(DbError::EntryNotFound) => {
                return Ok(Err(RequestError::GovernanceNotFound(
                    governance_id.to_str(),
                )))
            }
            Err(error) => return Err(InternalError::DatabaseError { source: error }),
        };
        if governance.subject_data.is_some() {
            let properties: Value =
                serde_json::from_str(&governance.subject_data.unwrap().properties)
                    .map_err(|_| InternalError::DeserializationError)?;
            let policies = get_as_array(&properties, "policies")?;
            let schema_policy = get_schema_from_policies(policies, &subject_data.schema_id);
            let Ok(schema_policy) = schema_policy else {
                return Ok(Err(schema_policy.unwrap_err()));
            };
            let approvers = get_as_array(&schema_policy.get("approval").unwrap(), "approvers")?;
            let all_approvers = get_members_from_set(&approvers);
            let Ok(all_approvers) = all_approvers else {
                return Ok(Err(all_approvers.unwrap_err()));
            };
            Ok(Ok(all_approvers))
        } else {
            Ok(Err(RequestError::GovernanceNotFound(
                governance_id.to_str(),
            )))
        }
    }

    fn parche(event: Event) -> Result<Result<HashSet<KeyIdentifier>, RequestError>, InternalError> {
        if event.event_content.metadata.governance_id.digest.is_empty() {
            if let State(state_request) = event.event_content.event_request.request {
                if let Json(properties_str) = state_request.payload {
                    let properties: Value = serde_json::from_str(&properties_str)
                        .map_err(|_| InternalError::DeserializationError)?;
                    let policies = get_as_array(&properties, "policies")?;
                    let schema_policy =
                        get_schema_from_policies(policies, &event.event_content.metadata.schema_id);
                    let Ok(schema_policy) = schema_policy else {
                        return Ok(Err(schema_policy.unwrap_err()));
                    };
                    let validators =
                        get_as_array(&schema_policy.get("validation").unwrap(), "validators")?;
                    let all_validators = get_members_from_set(&validators);
                    let Ok(all_validators) = all_validators else {
                        return Ok(Err(all_validators.unwrap_err()));
                    };
                    Ok(Ok(all_validators))
                } else {
                    Ok(Err(RequestError::UnexpectedPayloadType))
                }
            } else {
                Ok(Err(RequestError::InvalidRequestType))
            }
        } else {
            Ok(Err(RequestError::InvalidRequestType))
        }
    }

    pub fn check_policy(&self) -> Result<Result<bool, RequestError>, InternalError> {
        Ok(Ok(true))
    }

    pub fn get_governance_version(
        &self,
        governance_id: DigestIdentifier,
    ) -> Result<Result<u64, RequestError>, InternalError> {
        if governance_id.digest.is_empty() {
            return Ok(Ok(0));
        }
        let governance = match self.repo_access.get_subject(&governance_id) {
            Ok(governance) => governance,
            Err(DbError::EntryNotFound) => {
                return Ok(Err(RequestError::GovernanceNotFound(
                    governance_id.to_str(),
                )))
            }
            Err(error) => return Err(InternalError::DatabaseError { source: error }),
        };
        let subject_data = governance.subject_data.unwrap();
        if !subject_data.governance_id.digest.is_empty() {
            return Ok(Err(RequestError::InvalidGovernanceID));
        }
        Ok(Ok(subject_data.sn))
    }

    pub fn check_quorum(
        // TODO: Adapt
        &self,
        signers: HashSet<KeyIdentifier>,
        event: Event,
    ) -> Result<Result<(bool, HashSet<KeyIdentifier>), RequestError>, InternalError> {
        let governance_id = if event.event_content.metadata.governance_id.digest.is_empty() {
            event.event_content.subject_id.clone()
        } else {
            event.event_content.metadata.governance_id.clone()
        };
        let governance = match self.repo_access.get_subject(&governance_id) {
            Ok(governance) => governance,
            Err(DbError::EntryNotFound) => return self.parche2(signers, event),
            Err(error) => return Err(InternalError::DatabaseError { source: error }),
        };
        if governance.subject_data.is_none() {
            return self.parche2(signers, event);
        };
        let schema_id = event.event_content.metadata.schema_id.clone();
        let properties: Value = serde_json::from_str(&governance.subject_data.unwrap().properties)
            .map_err(|_| InternalError::DeserializationError)?;
        let policies = get_as_array(&properties, "policies")?;
        let schema_policy = get_schema_from_policies(policies, &schema_id);
        let Ok(schema_policy) = schema_policy else {
            return Ok(Err(schema_policy.unwrap_err()));
        };
        let validators = get_as_array(&schema_policy.get("validation").unwrap(), "validators")?;
        let all_validators = get_members_from_set(&validators);
        let Ok(all_validators) = all_validators else {
            return Ok(Err(all_validators.unwrap_err()));
        };
        let quorum_percentage = get_quorum(&schema_policy, "validation")?;
        let quorum = all_validators.len() as f64 * quorum_percentage;
        if signers
            .difference(&all_validators)
            .cloned()
            .collect::<HashSet<KeyIdentifier>>()
            .len()
            != 0
        {
            return Ok(Err(RequestError::InvalidKeyIdentifier(String::from(
                "One or more Signers are not valid validators",
            ))));
        };
        let remaining_signatures: HashSet<KeyIdentifier> =
            all_validators.difference(&signers).cloned().collect();
        return Ok(Ok((
            signers.len() >= quorum.ceil() as usize,
            remaining_signatures,
        )));
    }

    fn parche2(
        &self,
        signers: HashSet<KeyIdentifier>,
        event: Event,
    ) -> Result<Result<(bool, HashSet<KeyIdentifier>), RequestError>, InternalError> {
        if event.event_content.metadata.governance_id.digest.is_empty() {
            if let State(state_request) = event.event_content.event_request.request {
                if let Json(properties_str) = state_request.payload {
                    let properties: Value = serde_json::from_str(&properties_str)
                        .map_err(|_| InternalError::DeserializationError)?;
                    let schema_id = event.event_content.metadata.schema_id.clone();
                    let policies = get_as_array(&properties, "policies")?;
                    let schema_policy = get_schema_from_policies(policies, &schema_id);
                    let Ok(schema_policy) = schema_policy else {
                        return Ok(Err(schema_policy.unwrap_err()));
                    };
                    let validators =
                        get_as_array(&schema_policy.get("validation").unwrap(), "validators")?;
                    let all_validators = get_members_from_set(&validators);
                    let Ok(all_validators) = all_validators else {
                        return Ok(Err(all_validators.unwrap_err()));
                    };
                    let quorum_percentage = get_quorum(&schema_policy, "validation")?;
                    let quorum = all_validators.len() as f64 * quorum_percentage;
                    let remaining_signatures: HashSet<KeyIdentifier> =
                        all_validators.difference(&signers).cloned().collect();
                    return Ok(Ok((
                        signers.len() >= quorum.ceil() as usize,
                        remaining_signatures,
                    )));
                } else {
                    Ok(Err(RequestError::UnexpectedPayloadType))
                }
            } else {
                Ok(Err(RequestError::InvalidRequestType))
            }
        } else {
            Ok(Err(RequestError::InvalidRequestType))
        }
    }

    pub fn check_quorum_request(
        &self,
        event_request: EventRequest,
        approvals: HashSet<ApprovalResponse>,
    ) -> Result<Result<(RequestQuorum, HashSet<KeyIdentifier>), RequestError>, InternalError> {
        let EventRequestType::State(request) = event_request.request else {
            return Ok(Err(RequestError::InvalidRequestType))
        };
        let subject = match self.repo_access.get_subject(&request.subject_id) {
            Ok(subject) => subject,
            Err(DbError::EntryNotFound) => return Ok(Err(RequestError::SubjectNotFound)),
            Err(error) => return Err(InternalError::DatabaseError { source: error }),
        };
        let Some(subject_data) = subject.subject_data else {
            return Ok(Err(RequestError::SubjectNotFound))
        };
        let governance_id = if subject_data.governance_id.digest.is_empty() {
            subject_data.subject_id
        } else {
            subject_data.governance_id
        };
        let governance = match self.repo_access.get_subject(&governance_id) {
            Ok(governance) => governance,
            Err(DbError::EntryNotFound) => {
                return Ok(Err(RequestError::GovernanceNotFound(
                    governance_id.to_str(),
                )))
            }
            Err(error) => return Err(InternalError::DatabaseError { source: error }),
        };
        if governance.subject_data.is_none() {
            return Ok(Err(RequestError::GovernanceNotFound(
                governance_id.to_str(),
            )));
        };
        let properties: Value = serde_json::from_str(&governance.subject_data.unwrap().properties)
            .map_err(|_| InternalError::DeserializationError)?;

        let policies = get_as_array(&properties, "policies")?;
        let schema_policy = get_schema_from_policies(policies, &subject_data.schema_id);
        let Ok(schema_policy) = schema_policy else {
                return Ok(Err(schema_policy.unwrap_err()));
            };
        let approvers = get_as_array(&schema_policy.get("approval").unwrap(), "approvers")?;
        let all_approvers = get_members_from_set(&approvers);
        let Ok(all_approvers) = all_approvers else {
            return Ok(Err(all_approvers.unwrap_err()));
        };
        let quorum_percentage = get_quorum(&schema_policy, "approval")?;
        let signers: HashSet<KeyIdentifier> = approvals
            .clone()
            .into_iter()
            .map(|approval| approval.content.signer)
            .collect();
        if signers
            .difference(&all_approvers)
            .cloned()
            .collect::<HashSet<KeyIdentifier>>()
            .len()
            != 0
        {
            return Ok(Err(RequestError::InvalidKeyIdentifier(String::from(
                "One or more Signers are not valid approvers",
            ))));
        };
        let acceptance_quorum = (all_approvers.len() as f64 * quorum_percentage).ceil() as usize;
        let rejectance_quorum = all_approvers.len() + 1 - acceptance_quorum;
        let remaining_signatures: HashSet<KeyIdentifier> =
            all_approvers.difference(&signers).cloned().collect();
        let mut positive_approvals: usize = 0;
        let mut negative_approvals: usize = 0;
        for approval in approvals.into_iter() {
            match approval.content.approval_type {
                crate::commons::models::approval_signature::Acceptance::Accept => {
                    positive_approvals += 1
                }
                crate::commons::models::approval_signature::Acceptance::Reject => {
                    negative_approvals += 1
                }
            }
        }
        let quorum_result = if positive_approvals >= acceptance_quorum {
            RequestQuorum::Accepted
        } else if negative_approvals >= rejectance_quorum {
            RequestQuorum::Rejected
        } else {
            RequestQuorum::Processing
        };
        return Ok(Ok((quorum_result, remaining_signatures)));
    }

    pub fn is_governance(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<Result<bool, RequestError>, InternalError> {
        let subject = match self.repo_access.get_subject(&subject_id) {
            Ok(subject) => subject,
            Err(DbError::EntryNotFound) => return Ok(Err(RequestError::SubjectNotFound)),
            Err(error) => return Err(InternalError::DatabaseError { source: error }),
        };
        let Some(subject_data) = subject.subject_data else {
            return Ok(Err(RequestError::SubjectNotFound))
        };
        Ok(Ok(subject_data.governance_id.digest.is_empty()))
    }

    fn check_invokation(
        &self,
        properties: &Value,
        invokator: KeyIdentifier,
        owner: Option<String>,
        schema_id: &String,
    ) -> Result<Result<(bool, bool), RequestError>, InternalError> {
        let policies = get_as_array(&properties, "policies")?;
        let schema_policy = get_schema_from_policies(policies, &schema_id);
        let Ok(schema_policy) = schema_policy else {
            return Ok(Err(schema_policy.unwrap_err()));
        };
        let invokator_str = invokator.to_str();
        let invokation_rules = schema_policy.get("invokation").unwrap();
        let members = get_members_from_governance(&properties)?;
        Ok(Ok(is_valid_invokator(
            invokation_rules,
            &invokator_str,
            &owner,
            members,
        )?))
    }

    pub fn check_invokation_permission(
        &self,
        subject_id: DigestIdentifier,
        invokator: KeyIdentifier,
        additional_payload: Option<String>,
        metadata: Option<Metadata>,
    ) -> Result<Result<(bool, bool), RequestError>, InternalError> {
        let subject = match self.repo_access.get_subject(&subject_id) {
            Ok(subject) => subject,
            Err(DbError::EntryNotFound) => {
                if additional_payload.is_some() {
                    // Governance
                    let payload = additional_payload.unwrap();
                    let properties: Value = serde_json::from_str(&payload)
                        .map_err(|_| InternalError::DeserializationError)?;
                    return self.check_invokation(
                        &properties,
                        invokator,
                        Some(metadata.unwrap().owner.to_str()),
                        &"governance".into(),
                    );
                } else if metadata.is_some() {
                    let metadata = metadata.unwrap();
                    let governance_id = metadata.governance_id;
                    let governance = match self.repo_access.get_subject(&governance_id) {
                        Ok(governance) => governance,
                        Err(DbError::EntryNotFound) => {
                            return Ok(Err(RequestError::GovernanceNotFound(
                                governance_id.to_str(),
                            )))
                        }
                        Err(error) => return Err(InternalError::DatabaseError { source: error }),
                    };
                    if governance.subject_data.is_none() {
                        return Ok(Err(RequestError::GovernanceNotFound(
                            governance_id.to_str(),
                        )));
                    };
                    let properties: Value =
                        serde_json::from_str(&governance.subject_data.unwrap().properties)
                            .map_err(|_| InternalError::DeserializationError)?;
                    let owner = metadata.owner.to_str();
                    return self.check_invokation(
                        &properties,
                        invokator,
                        Some(owner),
                        &metadata.schema_id,
                    );
                }
                return Ok(Err(RequestError::SubjectNotFound));
            }
            Err(error) => return Err(InternalError::DatabaseError { source: error }),
        };
        let Some(subject_data) = subject.subject_data else {
            return Ok(Err(RequestError::SubjectNotFound));
        };
        let governance_id = if subject_data.governance_id.digest.is_empty() {
            subject_data.subject_id
        } else {
            subject_data.governance_id
        };
        let governance = match self.repo_access.get_subject(&governance_id) {
            Ok(governance) => governance,
            Err(DbError::EntryNotFound) => {
                return Ok(Err(RequestError::GovernanceNotFound(
                    governance_id.to_str(),
                )))
            }
            Err(error) => return Err(InternalError::DatabaseError { source: error }),
        };
        if governance.subject_data.is_none() {
            return Ok(Err(RequestError::GovernanceNotFound(
                governance_id.to_str(),
            )));
        };
        let properties: Value = serde_json::from_str(&governance.subject_data.unwrap().properties)
            .map_err(|_| InternalError::DeserializationError)?;
        let owner = subject_data.owner.to_str();
        self.check_invokation(&properties, invokator, Some(owner), &subject_data.schema_id)
    }
}

fn get_as_str<'a>(data: &'a Value, key: &str) -> Result<&'a str, InternalError> {
    data.get(key)
        .ok_or(InternalError::InvalidGovernancePayload)?
        .as_str()
        .ok_or(InternalError::InvalidGovernancePayload)
}

fn get_as_array<'a>(data: &'a Value, key: &str) -> Result<&'a Vec<Value>, InternalError> {
    data.get(key)
        .ok_or(InternalError::InvalidGovernancePayload)?
        .as_array()
        .ok_or(InternalError::InvalidGovernancePayload)
}

fn get_schema_from_policies<'a>(
    data: &'a Vec<Value>,
    key: &str,
) -> Result<&'a Value, RequestError> {
    data.iter()
        .find(|&policy| {
            let id = policy.get("id").unwrap().as_str().unwrap();
            id == key
        })
        .ok_or(RequestError::SchemaNotFoundInPolicies)
}

fn get_members_from_set<'a>(data: &'a Vec<Value>) -> Result<HashSet<KeyIdentifier>, RequestError> {
    let mut all_validators = HashSet::new();
    for member in data {
        let string_id = member.as_str().unwrap();
        let key = KeyIdentifier::from_str(string_id);
        if key.is_err() {
            return Err(RequestError::InvalidKeyIdentifier(String::from(string_id)));
        }
        all_validators.insert(key.unwrap());
    }
    Ok(all_validators)
}

fn get_quorum<'a>(data: &'a Value, key: &str) -> Result<f64, InternalError> {
    data.get(key)
        .ok_or(InternalError::InvalidGovernancePayload)?
        .get("quorum")
        .ok_or(InternalError::InvalidGovernancePayload)?
        .as_f64()
        .ok_or(InternalError::InvalidGovernancePayload)
}

fn is_valid_invokator(
    invokation_rules: &Value,
    invokator: &String,
    owner: &Option<String>,
    members: HashSet<String>,
) -> Result<(bool, bool), InternalError> {
    // Checking owner rule
    if let Some(owner) = owner {
        if invokator == owner {
            let owner_rule = invokation_rules
                .get("owner")
                .ok_or(InternalError::InvalidGovernancePayload)?;
            return Ok(extract_allowance_and_approval_required(owner_rule)?);
        }
    }
    // Chacking set rule
    let set_rule = invokation_rules
        .get("set")
        .ok_or(InternalError::InvalidGovernancePayload)?;
    let set_rule_invokers = get_as_array(set_rule, "invokers")?
        .iter()
        .find(|&id| id.as_str().is_some() && id.as_str().unwrap() == invokator);
    if let Some(_set_rule_invokers) = set_rule_invokers {
        Ok(extract_allowance_and_approval_required(set_rule)?)
    } else if members.contains(invokator) {
        // Checking all rule
        let all_rule = invokation_rules
            .get("all")
            .ok_or(InternalError::InvalidGovernancePayload)?;
        Ok(extract_allowance_and_approval_required(all_rule)?)
    } else {
        // Checking external rule
        let external_rule = invokation_rules
            .get("external")
            .ok_or(InternalError::InvalidGovernancePayload)?;
        Ok(extract_allowance_and_approval_required(external_rule)?)
    }
}

fn extract_allowance_and_approval_required(
    invokation_rule: &Value,
) -> Result<(bool, bool), InternalError> {
    let allowance = invokation_rule
        .get("allowance")
        .ok_or(InternalError::InvalidGovernancePayload)?
        .as_bool()
        .ok_or(InternalError::InvalidGovernancePayload)?;
    let approval_required = invokation_rule
        .get("approvalRequired")
        .ok_or(InternalError::InvalidGovernancePayload)?
        .as_bool()
        .ok_or(InternalError::InvalidGovernancePayload)?;
    Ok((allowance, approval_required))
}

fn get_members_from_governance(properties: &Value) -> Result<HashSet<String>, InternalError> {
    let mut member_ids: HashSet<String> = HashSet::new();
    let members = properties
        .get("members")
        .unwrap()
        .as_array()
        .unwrap()
        .to_owned();
    for member in members.into_iter() {
        let member_id = member
            .get("key")
            .expect("Se ha validado que tiene key")
            .as_str()
            .expect("Hay id y es str")
            .to_owned();
        let true = member_ids.insert(member_id) else {
            return Err(InternalError::InvalidGovernancePayload);
        };
    }
    Ok(member_ids)
}
