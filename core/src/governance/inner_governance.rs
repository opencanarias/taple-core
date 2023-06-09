use std::{collections::HashSet, str::FromStr};

use crate::{
    commons::{
        self,
        errors::ChannelErrors,
        identifier::{Derivable, DigestIdentifier, KeyIdentifier},
        models::{event_content::Metadata, state::Subject},
        schema_handler::{
            gov_models::{Contract, Quorum, Role, Schema},
            initial_state::get_governance_initial_state,
        },
    },
    database::Error as DbError,
};
use serde_json::Value;

use super::{
    error::{InternalError, RequestError},
    stage::ValidationStage,
    GovernanceUpdatedMessage,
};

use crate::database::{DatabaseCollection, DB};

pub struct InnerGovernance<C: DatabaseCollection> {
    repo_access: DB<C>,
    governance_schema: Value,
    update_channel: tokio::sync::broadcast::Sender<GovernanceUpdatedMessage>,
}

impl<C: DatabaseCollection> InnerGovernance<C> {
    pub fn new(
        repo_access: DB<C>,
        governance_schema: Value,
        update_channel: tokio::sync::broadcast::Sender<GovernanceUpdatedMessage>,
    ) -> InnerGovernance<C> {
        Self {
            repo_access,
            governance_schema,
            update_channel,
        }
    }

    pub fn get_roles_of_invokator(
        &self,
        invokator: KeyIdentifier,
        metadata: Metadata,
    ) -> Result<Result<Vec<String>, RequestError>, InternalError> {
        let mut governance_id = metadata.governance_id;
        if governance_id.digest.is_empty() {
            governance_id = metadata.subject_id;
        }
        let schema_id = metadata.schema_id;
        let governance =
            match self.governance_event_sourcing(&governance_id, metadata.governance_version) {
                Ok(subject) => subject,
                Err(error) => match error {
                    RequestError::DatabaseError(err) => {
                        return Err(InternalError::DatabaseError { source: err })
                    }
                    err => return Ok(Err(err)),
                },
            };
        let properties: Value = serde_json::from_str(&governance.properties)
            .map_err(|_| InternalError::DeserializationError)?;
        let roles_prop = properties["roles"]
            .as_array()
            .expect("Existe roles")
            .to_owned();
        let roles = get_roles(&schema_id, roles_prop, &metadata.namespace)?;
        let members = get_members_from_governance(&properties)?;
        let invokator_roles = get_invokator_roles(&invokator, &members, roles)?;
        Ok(Ok(invokator_roles))
    }

    // NEW
    pub fn get_init_state(
        &self,
        governance_id: DigestIdentifier,
        schema_id: String,
        governance_version: u64,
    ) -> Result<Result<Value, RequestError>, InternalError> {
        if governance_id.digest.is_empty() {
            return Ok(Ok(get_governance_initial_state()));
        }
        let governance = match self.governance_event_sourcing(&governance_id, governance_version) {
            Ok(subject) => subject,
            Err(error) => match error {
                RequestError::DatabaseError(err) => {
                    return Err(InternalError::DatabaseError { source: err })
                }
                err => return Ok(Err(err)),
            },
        };
        let properties: Value = serde_json::from_str(&governance.properties)
            .map_err(|_| InternalError::DeserializationError)?;
        let schemas = get_as_array(&properties, "schemas")?;
        for schema in schemas {
            let tmp = get_as_str(schema, "id")?;
            if tmp == &schema_id {
                return Ok(Ok(schema.get("initial_value").unwrap().to_owned()));
            }
        }
        return Ok(Err(RequestError::SchemaNotFound(schema_id)));
    }

    // UPDATED
    pub fn get_schema(
        &self,
        governance_id: DigestIdentifier,
        schema_id: String,
        governance_version: u64,
    ) -> Result<Result<Value, RequestError>, InternalError> {
        if governance_id.digest.is_empty() {
            return Ok(Ok(self.governance_schema.clone()));
        }
        let governance = match self.governance_event_sourcing(&governance_id, governance_version) {
            Ok(subject) => subject,
            Err(error) => match error {
                RequestError::DatabaseError(err) => {
                    return Err(InternalError::DatabaseError { source: err })
                }
                err => return Ok(Err(err)),
            },
        };
        let properties: Value = serde_json::from_str(&governance.properties)
            .map_err(|_| InternalError::DeserializationError)?;
        let schemas = get_as_array(&properties, "schemas")?;
        for schema in schemas {
            let tmp = get_as_str(schema, "id")?;
            if tmp == &schema_id {
                return Ok(Ok(schema.get("schema").unwrap().to_owned()));
            }
        }
        return Ok(Err(RequestError::SchemaNotFound(schema_id)));
    }

    fn get_signers_aux(
        properties: &Value,
        schema_id: &str,
        metadata: &Metadata,
        signers_roles: &Vec<String>,
    ) -> Result<Result<HashSet<KeyIdentifier>, RequestError>, InternalError> {
        let members = get_members_from_governance(&properties)?;
        let roles_prop = properties["roles"]
            .as_array()
            .expect("Existe roles")
            .to_owned();
        let roles = get_roles(&schema_id, roles_prop, &metadata.namespace)?;
        let signers = get_signers_from_roles(&members, signers_roles, roles)?;
        Ok(Ok(signers))
    }

    // NEW
    // Cuando se piden testigos de incluyen los aprobadores actualmente
    pub fn get_signers(
        &self,
        metadata: Metadata,
        stage: ValidationStage,
    ) -> Result<Result<HashSet<KeyIdentifier>, RequestError>, InternalError> {
        let mut governance_id = metadata.governance_id.clone();
        if governance_id.digest.is_empty() {
            governance_id = metadata.subject_id.clone();
        }
        let schema_id = metadata.schema_id.clone();
        let governance =
            match self.governance_event_sourcing(&governance_id, metadata.governance_version) {
                Ok(subject) => subject,
                Err(error) => match error {
                    RequestError::DatabaseError(err) => {
                        return Err(InternalError::DatabaseError { source: err })
                    }
                    err => return Ok(Err(err)),
                },
            };
        let properties: Value = serde_json::from_str(&governance.properties)
            .map_err(|_| InternalError::DeserializationError)?;
        let policies = get_as_array(&properties, "policies")?;
        let schema_policy = get_schema_from_policies(policies, &schema_id);
        let Ok(schema_policy) = schema_policy else {
            return Ok(Err(schema_policy.unwrap_err()));
        }; // El return dentro de otro return es una **** que obliga a hacer cosas como esta
        match stage {
            ValidationStage::Approve | ValidationStage::Evaluate | ValidationStage::Validate => {
                let stage_str = stage.to_str();
                let roles: Vec<String> =
                    get_as_array(&schema_policy.get(stage_str).unwrap(), "roles")?
                        .into_iter()
                        .map(|role| {
                            role.as_str()
                                .ok_or(InternalError::InvalidGovernancePayload("0".into()))
                                .map(|s| s.to_owned())
                                .expect("Invalid Governance Payload")
                        })
                        .collect();
                Self::get_signers_aux(&properties, &schema_id, &metadata, &roles)
            }
            ValidationStage::Witness => {
                // Todos los aprobadores son también testigos, así que deben traerse sus roles también
                let stage_str = stage.to_str();
                let approvers_roles: Vec<String> = get_as_array(&schema_policy, stage_str)?
                    .into_iter()
                    .map(|role| {
                        let a = role
                            .as_str()
                            .ok_or(InternalError::InvalidGovernancePayload("1".into()))
                            .map(|s| s.to_owned());
                        a.expect("Invalid Governance Payload")
                    })
                    .collect();
                let witness_roles: Vec<String> = get_as_array(
                    &schema_policy
                        .get(ValidationStage::Approve.to_str())
                        .unwrap(),
                    "roles",
                )?
                .into_iter()
                .map(|role| {
                    let a = role
                        .as_str()
                        .ok_or(InternalError::InvalidGovernancePayload("2".into()))
                        .map(|s| s.to_owned());
                    a.expect("Invalid Governance Payload")
                })
                .collect();
                let mut set: HashSet<String> = HashSet::new();
                for s in approvers_roles.into_iter().chain(witness_roles.into_iter()) {
                    set.insert(s);
                }
                let signers_roles: Vec<String> = set.into_iter().collect();
                Self::get_signers_aux(&properties, &schema_id, &metadata, &signers_roles)
            }
            ValidationStage::Close | ValidationStage::Create | ValidationStage::Invoke => {
                let stage_str = stage.to_str();
                let roles: Vec<String> = get_as_array(&schema_policy, stage_str)?
                    .into_iter()
                    .map(|role| {
                        role.as_str()
                            .ok_or(InternalError::InvalidGovernancePayload("3".into()))
                            .map(|s| s.to_owned())
                            .expect("Invalid Governance Payload")
                    })
                    .collect();
                Self::get_signers_aux(&properties, &schema_id, &metadata, &roles)
            }
        }
    }

    // NEW Devuelve el número de firmas necesarias para que un evento sea válido
    pub fn get_quorum(
        &self,
        metadata: Metadata,
        stage: ValidationStage,
    ) -> Result<Result<u32, RequestError>, InternalError> {
        let mut governance_id = metadata.governance_id;
        log::info!("Quorum de: {}", metadata.subject_id.to_str());
        if governance_id.digest.is_empty() {
            governance_id = metadata.subject_id;
        }
        let schema_id = metadata.schema_id;
        let governance =
            match self.governance_event_sourcing(&governance_id, metadata.governance_version) {
                Ok(subject) => subject,
                Err(error) => match error {
                    RequestError::DatabaseError(err) => {
                        return Err(InternalError::DatabaseError { source: err })
                    }
                    err => return Ok(Err(err)),
                },
            };
        let properties: Value = serde_json::from_str(&governance.properties)
            .map_err(|_| InternalError::DeserializationError)?;
        let policies = get_as_array(&properties, "policies")?;
        let schema_policy = get_schema_from_policies(policies, &schema_id);
        let Ok(schema_policy) = schema_policy else {
            return Ok(Err(schema_policy.unwrap_err()));
        }; // El return dentro de otro return es una **** que obliga a hacer cosas como esta
        let stage_str = stage.to_str();
        let signers_roles: Vec<String> =
            get_as_array(&schema_policy.get(stage_str).unwrap(), "roles")?
                .into_iter()
                .map(|role| {
                    let a = role
                        .as_str()
                        .ok_or(InternalError::InvalidGovernancePayload("4".into()))
                        .map(|s| s.to_owned());
                    a.expect("Invalid Governance Payload")
                })
                .collect();
        let members = get_members_from_governance(&properties)?;
        let roles_prop = properties["roles"]
            .as_array()
            .expect("Existe roles")
            .to_owned();
        let roles = get_roles(&schema_id, roles_prop, &metadata.namespace)?;
        let signers = get_signers_from_roles(&members, &signers_roles, roles)?;
        let quorum = get_quorum(&schema_policy, stage_str)?;
        match quorum {
            Quorum::MAJORITY(_) => {
                log::info!("Quorum Majority");

                Ok(Ok((signers.len() as u32 / 2) + 1))
            }
            Quorum::FIXED { fixed } => {
                log::info!("Quorum fijo: {}", fixed);
                Ok(Ok(fixed))
            }
            Quorum::PORCENTAJE { porcentaje } => {
                let result = (signers.len() as f64 * porcentaje).ceil() as u32;
                Ok(Ok(result))
            }
            Quorum::BFT { BFT } => todo!(),
        }
    }

    // NEW
    pub fn get_invoke_info(
        &self,
        metadata: Metadata,
        fact: &str,
    ) -> Result<Result<HashSet<KeyIdentifier>, RequestError>, InternalError> {
        let mut governance_id = metadata.governance_id;
        if governance_id.digest.is_empty() {
            governance_id = metadata.subject_id;
        }
        let schema_id = metadata.schema_id;
        let governance =
            match self.governance_event_sourcing(&governance_id, metadata.governance_version) {
                Ok(subject) => subject,
                Err(error) => match error {
                    RequestError::DatabaseError(err) => {
                        return Err(InternalError::DatabaseError { source: err })
                    }
                    err => return Ok(Err(err)),
                },
            };
        let properties: Value = serde_json::from_str(&governance.properties)
            .map_err(|_| InternalError::DeserializationError)?;
        let policies = get_as_array(&properties, "policies")?;
        let schema_policy = get_schema_from_policies(policies, &schema_id);
        let Ok(schema_policy) = schema_policy else {
            return Ok(Err(schema_policy.unwrap_err()));
        }; // El return dentro de otro return es una **** que obliga a hacer cosas como esta
           // Se puede refactorizar lo de arriba de aquí y meter en una función porque es lo mismo en todos los métodos nuevos
        todo!();
        // Ok(Ok(invoke))
    }

    // NEW
    pub fn get_contracts(
        &self,
        governance_id: DigestIdentifier,
        governance_version: u64,
    ) -> Result<Result<Vec<Contract>, RequestError>, InternalError> {
        let governance = match self.governance_event_sourcing(&governance_id, governance_version) {
            Ok(subject) => subject,
            Err(error) => match error {
                RequestError::DatabaseError(err) => {
                    return Err(InternalError::DatabaseError { source: err })
                }
                err => return Ok(Err(err)),
            },
        };
        let properties: Value = serde_json::from_str(&governance.properties)
            .map_err(|_| InternalError::DeserializationError)?;
        let schemas = get_as_array(&properties, "schemas")?;
        let mut result = Vec::new();
        for schema in schemas {
            let mut contract: Contract = serde_json::from_value(schema["contract"].clone())
                .map_err(|_| InternalError::InvalidGovernancePayload("5".into()))?;
            let decoded_bytes =
                base64::decode(contract.content).map_err(|_| InternalError::Base64DecodingError)?;
            contract.content =
                String::from_utf8(decoded_bytes).map_err(|_| InternalError::Base64DecodingError)?;
            result.push(contract);
        }
        Ok(Ok(result))
    }

    // OLD BUT OK
    pub fn get_governance_version(
        &self,
        subject_id: DigestIdentifier,
        governance_id: DigestIdentifier,
    ) -> Result<Result<u64, RequestError>, InternalError> {
        let governance_id = if governance_id.digest.is_empty() {
            subject_id
        } else {
            governance_id
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
        if &governance.governance_id.to_str() != "" {
            return Ok(Err(RequestError::InvalidGovernanceID));
        }
        Ok(Ok(governance.sn))
    }

    // OLD pero puede valer
    pub fn is_governance(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<Result<bool, RequestError>, InternalError> {
        let subject = match self.repo_access.get_subject(&subject_id) {
            Ok(subject) => subject,
            Err(DbError::EntryNotFound) => return Ok(Err(RequestError::SubjectNotFound)),
            Err(error) => return Err(InternalError::DatabaseError { source: error }),
        };
        Ok(Ok(subject.governance_id.digest.is_empty()))
    }

    pub async fn governance_updated(
        &self,
        governance_id: DigestIdentifier,
        governance_version: u64,
    ) -> Result<Result<(), RequestError>, InternalError> {
        self.update_channel
            .send(GovernanceUpdatedMessage::GovernanceUpdated {
                governance_id: governance_id.clone(),
                governance_version,
            })
            .map_err(|e| InternalError::ChannelError {
                source: ChannelErrors::ChannelClosed,
            })?;
        Ok(Ok(()))
    }

    fn governance_event_sourcing(
        &self,
        governance_id: &DigestIdentifier,
        governance_version: u64,
    ) -> Result<Subject, RequestError> {
        let gov_subject = match self.repo_access.get_subject(governance_id) {
            Ok(governance) => governance,
            Err(DbError::EntryNotFound) => {
                return Err(RequestError::GovernanceNotFound(governance_id.to_str()))
            }
            Err(error) => return Err(RequestError::DatabaseError(error)),
        };
        if gov_subject.sn == governance_version {
            Ok(gov_subject)
        } else if gov_subject.sn > governance_version {
            let gov_genesis = self.repo_access.get_event(governance_id, 0)?;
            let init_state = get_governance_initial_state();
            let init_state = serde_json::to_string(&init_state)
                .map_err(|_| RequestError::ErrorParsingJsonString("Init state".to_owned()))?;
            let mut gov_subject = Subject::from_genesis_event(gov_genesis, init_state)?;
            for i in 1..=governance_version {
                let event = self.repo_access.get_event(governance_id, i)?;
                gov_subject.update_subject(&event.content.event_proposal.proposal.json_patch, i)?;
            }
            Ok(gov_subject)
        } else {
            Err(RequestError::GovernanceVersionTooHigh(
                governance_id.to_str(),
                governance_version,
            ))
        }
    }
}

fn get_as_str<'a>(data: &'a Value, key: &str) -> Result<&'a str, InternalError> {
    data.get(key)
        .ok_or(InternalError::InvalidGovernancePayload("6".into()))?
        .as_str()
        .ok_or(InternalError::InvalidGovernancePayload("7".into()))
}

fn get_as_array<'a>(data: &'a Value, key: &str) -> Result<&'a Vec<Value>, InternalError> {
    data.get(key)
        .ok_or(InternalError::InvalidGovernancePayload("8".into()))?
        .as_array()
        .ok_or(InternalError::InvalidGovernancePayload("9".into()))
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

fn get_quorum<'a>(data: &'a Value, key: &str) -> Result<Quorum, InternalError> {
    let json_data = data
        .get(key)
        .ok_or(InternalError::InvalidGovernancePayload("10".into()))?
        .get("quorum")
        .ok_or(InternalError::InvalidGovernancePayload("11".into()))?;
    log::warn!("QUORUM: {:?}", json_data);
    let quorum: Quorum = serde_json::from_value(json_data.clone()).unwrap();
    log::warn!("QUORUM: {:?}", quorum);
    Ok(quorum)
}

fn get_members_from_governance(
    properties: &Value,
) -> Result<HashSet<KeyIdentifier>, InternalError> {
    let mut member_ids: HashSet<KeyIdentifier> = HashSet::new();
    let members = properties
        .get("members")
        .unwrap()
        .as_array()
        .unwrap()
        .to_owned();
    for member in members.into_iter() {
        let member_id = member
            .get("id")
            .expect("Se ha validado que tiene id")
            .as_str()
            .expect("Hay id y es str");
        let member_id = KeyIdentifier::from_str(member_id)
            .map_err(|_| InternalError::InvalidGovernancePayload("12".into()))?;
        let true = member_ids.insert(member_id) else {
            return Err(InternalError::InvalidGovernancePayload("13".into()));
        };
    }
    Ok(member_ids)
}

fn get_signers_from_roles(
    members: &HashSet<KeyIdentifier>,
    roles: &Vec<String>,
    roles_schema: Vec<Role>,
) -> Result<HashSet<KeyIdentifier>, InternalError> {
    let mut signers: HashSet<KeyIdentifier> = HashSet::new();
    for role in roles_schema {
        if roles.contains(&role.role) {
            match role.who {
                crate::commons::schema_handler::gov_models::Who::ID { ID } => {
                    signers.insert(KeyIdentifier::from_str(&ID).map_err(|_| InternalError::InvalidGovernancePayload("14".into()))?);
                }
                crate::commons::schema_handler::gov_models::Who::MEMBERS => {
                    return Ok(members.clone())
                }
                _ => {
                }
                // Entiendo que con esto no se hace nada de cara a validación
            }
        }
    }
    for signer in signers.iter() {
        log::warn!("FINAL SIGNERS: {}", signer.to_str());
    }
    Ok(signers)
}

fn get_roles(
    schema_id: &str,
    roles_prop: Vec<Value>,
    namespace: &str,
) -> Result<Vec<Role>, InternalError> {
    let mut roles = Vec::new();
    for role in roles_prop {
        let role_data: Role = serde_json::from_value(role)
            .map_err(|_| InternalError::InvalidGovernancePayload("15".into()))?;
        if !namespace_contiene(&role_data.namespace, namespace) {
            continue;
        }
        match &role_data.schema {
            Schema::ID { ID } => {
                if &ID == &schema_id {
                    roles.push(role_data)
                }
            }
            Schema::AllSchemas => roles.push(role_data),
        }
    }
    Ok(roles)
}

fn get_invokator_roles(
    invokator: &KeyIdentifier,
    members: &HashSet<KeyIdentifier>,
    roles_schema: Vec<Role>,
) -> Result<Vec<String>, InternalError> {
    let mut roles = Vec::new();
    let mut is_external = false;
    if !members.contains(invokator) {
        is_external = true;
    }
    let invokator_str = invokator.to_str();
    for role in roles_schema {
        match role.who {
            crate::commons::schema_handler::gov_models::Who::ID { ID } => {
                if !is_external && &ID == &invokator_str {
                    roles.push(role.role);
                }
            }
            crate::commons::schema_handler::gov_models::Who::MEMBERS => {
                if !is_external {
                    roles.push(role.role);
                }
            }
            crate::commons::schema_handler::gov_models::Who::ALL => {
                roles.push(role.role);
            }
            crate::commons::schema_handler::gov_models::Who::NOT_MEMBERS => {
                if is_external {
                    roles.push(role.role);
                }
            }
        }
    }
    Ok(roles)
}

fn contains_common_element(set1: &HashSet<String>, vec2: &[String]) -> bool {
    vec2.iter().any(|s| set1.contains(s))
}

fn namespace_contiene(namespace_padre: &str, namespace_hijo: &str) -> bool {
    // Si el namespace padre es vacío, contiene a todos
    if namespace_padre.is_empty() {
        return true;
    }

    // Si el namespace padre y el namespace hijo son iguales, entonces contiene
    if namespace_padre == namespace_hijo {
        return true;
    }

    // Asegurarse de que el namespace hijo comienza con el namespace padre
    if !namespace_hijo.starts_with(namespace_padre) {
        return false;
    }

    // Verificar si el namespace padre contiene al hijo como subnamespace
    if let Some(remaining) = namespace_hijo.strip_prefix(namespace_padre) {
        // El primer carácter después del prefijo del namespace padre debe ser un punto
        return remaining.starts_with(".");
    }

    false
}
