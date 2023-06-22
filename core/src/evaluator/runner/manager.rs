use json_patch::diff;
use serde_json::Value;
use wasmtime::Engine;

use crate::{
    commons::{
        models::{event_preevaluation::EventPreEvaluation, Acceptance},
        schema_handler::Schema,
    },
    database::DB,
    evaluator::errors::ExecutorErrorResponses,
    event_request::FactRequest,
    governance::GovernanceInterface,
    identifier::DigestIdentifier,
    DatabaseCollection, EventRequestType, ValueWrapper,
};

use super::{
    executor::{ContractExecutor, ContractResult},
    ExecuteContractResponse,
};
use crate::database::Error as DbError;
pub struct TapleRunner<C: DatabaseCollection, G: GovernanceInterface> {
    database: DB<C>,
    executor: ContractExecutor,
    gov_api: G,
}

impl<C: DatabaseCollection, G: GovernanceInterface> TapleRunner<C, G> {
    pub fn new(database: DB<C>, engine: Engine, gov_api: G) -> Self {
        Self {
            database,
            executor: ContractExecutor::new(engine),
            gov_api,
        }
    }

    pub fn generate_context_hash(
        execute_contract: &EventPreEvaluation,
    ) -> Result<DigestIdentifier, ExecutorErrorResponses> {
        DigestIdentifier::from_serializable_borsh(execute_contract)
            .map_err(|_| ExecutorErrorResponses::ContextHashGenerationFailed)
    }

    pub async fn execute_contract(
        &self,
        execute_contract: &EventPreEvaluation,
        state_data: &FactRequest,
    ) -> Result<ExecuteContractResponse, ExecutorErrorResponses> {
        // Check governance version
        let governance_id = if &execute_contract.context.schema_id == "governance" {
            if let EventRequestType::Fact(data) = &execute_contract.event_request.request {
                data.subject_id.clone()
            } else {
                return Err(ExecutorErrorResponses::CreateRequestNotAllowed);
            }
        } else {
            execute_contract.context.governance_id.clone()
        };
        let governance = match self.database.get_subject(&governance_id) {
            Ok(governance) => governance,
            Err(DbError::EntryNotFound) => {
                // Pedimos LCE
                return Err(ExecutorErrorResponses::OurGovIsLower);
            }
            Err(error) => return Err(ExecutorErrorResponses::DatabaseError(error.to_string())),
        };
        if governance.sn > execute_contract.context.governance_version {
            // Nuestra gov es mayor: mandamos mensaje para que actualice el emisor
            return Err(ExecutorErrorResponses::OurGovIsHigher);
        } else if governance.sn < execute_contract.context.governance_version {
            // Nuestra gov es menor: no podemos hacer nada. Pedimos LCE al que nos lo envió
            return Err(ExecutorErrorResponses::OurGovIsLower);
        }
        let context_hash = Self::generate_context_hash(execute_contract)?;
        let (contract, governance_version) = if execute_contract.context.schema_id == "governance"
            && execute_contract.context.governance_id.digest.is_empty()
        {
            match self.database.get_governance_contract() {
                // TODO: Gestionar versión gobernanza
                Ok(contract) => (contract, execute_contract.context.governance_version),
                Err(DbError::EntryNotFound) => {
                    let governance_version = match self
                        .database
                        .get_subject(&execute_contract.context.governance_id)
                    {
                        Ok(governance) => governance.sn,
                        Err(DbError::EntryNotFound) => 0,
                        Err(error) => {
                            return Err(ExecutorErrorResponses::DatabaseError(error.to_string()))
                        }
                    };
                    return Ok(ExecuteContractResponse {
                        json_patch: serde_json::from_str("")
                            .map_err(|_| ExecutorErrorResponses::JSONPATCHDeserializationFailed)?,
                        hash_new_state: DigestIdentifier::default(),
                        governance_version,
                        context_hash,
                        success: Acceptance::Ko,
                        approval_required: false,
                    });
                }
                Err(error) => return Err(ExecutorErrorResponses::DatabaseError(error.to_string())),
            }
        } else {
            match self.database.get_contract(
                &execute_contract.context.governance_id,
                &execute_contract.context.schema_id,
            ) {
                Ok((contract, _, governance_version)) => (contract, governance_version),
                Err(DbError::EntryNotFound) => {
                    let governance_version = match self
                        .database
                        .get_subject(&execute_contract.context.governance_id)
                    {
                        Ok(governance) => governance.sn,
                        Err(DbError::EntryNotFound) => 0,
                        Err(error) => {
                            return Err(ExecutorErrorResponses::DatabaseError(error.to_string()))
                        }
                    };
                    return Ok(ExecuteContractResponse {
                        json_patch: serde_json::from_str("")
                            .map_err(|_| ExecutorErrorResponses::JSONPATCHDeserializationFailed)?,
                        hash_new_state: DigestIdentifier::default(),
                        governance_version,
                        context_hash,
                        success: Acceptance::Ko,
                        approval_required: false,
                    });
                }
                Err(error) => return Err(ExecutorErrorResponses::DatabaseError(error.to_string())),
            }
        };
        let previous_state = &execute_contract.context.actual_state.clone();
        let mut contract_result = self
            .executor
            .execute_contract(
                &execute_contract.context.actual_state,
                &state_data.payload,
                contract,
                &execute_contract.event_request.signature.content.signer
                    == &execute_contract.context.owner,
            )
            .await?;
        let (patch, hash) = match contract_result.success {
            Acceptance::Ok => {
                match self
                    .validation_state(
                        &contract_result,
                        &governance_id,
                        execute_contract.context.schema_id.clone(),
                        execute_contract.context.governance_version,
                    )
                    .await
                {
                    Ok(false) | Err(_) => {
                        contract_result.success = Acceptance::Ko;
                        (
                            serde_json::from_str("").map_err(|_| {
                                ExecutorErrorResponses::JSONPATCHDeserializationFailed
                            })?,
                            DigestIdentifier::default(),
                        )
                    }
                    _ => (
                        generate_json_patch(&previous_state.0, &contract_result.final_state.0)?,
                        DigestIdentifier::from_serializable_borsh(&contract_result.final_state)
                            .map_err(|_| ExecutorErrorResponses::StateHashGenerationFailed)?,
                    ),
                }
            }
            Acceptance::Ko => (
                serde_json::from_str("")
                    .map_err(|_| ExecutorErrorResponses::JSONPATCHDeserializationFailed)?,
                DigestIdentifier::default(),
            ),
        };
        Ok(ExecuteContractResponse {
            json_patch: ValueWrapper(patch),
            hash_new_state: hash,
            governance_version,
            context_hash,
            success: contract_result.success,
            approval_required: contract_result.approval_required,
        })
    }

    async fn validation_state(
        &self,
        contract_result: &ContractResult,
        governance_id: &DigestIdentifier,
        schema_id: String,
        governance_version: u64,
    ) -> Result<bool, ExecutorErrorResponses> {
        if Acceptance::Ok == contract_result.success {
            let new_state = &contract_result.final_state;
            // Comprobar el estado contra el esquema definido en la gobernanza
            let schema = self
                .gov_api
                .get_schema(governance_id.clone(), schema_id, governance_version)
                .await?;
            let schema = Schema::compile(&schema.0)
                .map_err(|_| ExecutorErrorResponses::SchemaCompilationFailed)?;
            Ok(schema.validate(
                &serde_json::to_value(new_state)
                    .map_err(|_| ExecutorErrorResponses::StateJSONDeserializationFailed)?,
            ))
        } else {
            Ok(true)
        }
    }
}

fn generate_json_patch(
    prev_state: &Value,
    new_state: &Value,
) -> Result<Value, ExecutorErrorResponses> {
    let patch = diff(&prev_state, &new_state);
    Ok(serde_json::to_value(&patch)
        .map_err(|_| ExecutorErrorResponses::JSONPATCHDeserializationFailed)?)
}
