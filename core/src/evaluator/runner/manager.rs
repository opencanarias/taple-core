use json_patch::diff;
use serde_json::Value;
use wasmtime::Engine;

use crate::{
    commons::models::{event_preevaluation::EventPreEvaluation, Acceptance},
    database::DB,
    evaluator::errors::ExecutorErrorResponses,
    event_request::StateRequest,
    governance::GovernanceInterface,
    identifier::DigestIdentifier,
    DatabaseCollection
};

use super::{executor::ContractExecutor, ExecuteContractResponse};
use crate::database::Error as DbError;
pub struct TapleRunner<C: DatabaseCollection, G: GovernanceInterface + Send> {
    database: DB<C>,
    executor: ContractExecutor<G>,
}

impl<C: DatabaseCollection, G: GovernanceInterface + Send> TapleRunner<C, G> {
    pub fn new(database: DB<C>, engine: Engine, gov_api: G) -> Self {
        Self {
            database,
            executor: ContractExecutor::new(engine, gov_api),
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
        state_data: &StateRequest,
    ) -> Result<ExecuteContractResponse, ExecutorErrorResponses> {
        // Check governance version
        let governance = self
            .database
            .get_subject(&execute_contract.context.governance_id)
            .map_err(|db_err| ExecutorErrorResponses::DatabaseError(db_err.to_string()))?;
        if governance.sn > execute_contract.context.governance_version {
            // Nuestra gov es mayor: mandamos mensaje para que actualice el emisor
            return Err(ExecutorErrorResponses::OurGovIsHigher)
        } else if governance.sn < execute_contract.context.governance_version {
            // Nuestra gov es menor: no podemos hacer nada. Pedimos LCE al que nos lo envió
            return Err(ExecutorErrorResponses::OurGovIsLower)
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
                        json_patch: String::from(""),
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
                        json_patch: String::from(""),
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
        let previous_state = execute_contract.context.actual_state.clone();
        let contract_result = self
            .executor
            .execute_contract(
                &execute_contract.context.actual_state,
                &state_data.invokation,
                &execute_contract.event_request.signature.content.signer,
                &execute_contract.context,
                governance_version,
                contract,
                &state_data.subject_id,
            )
            .await?;
        let (patch, hash) = match contract_result.success {
            Acceptance::Ok => (
                generate_json_patch(&previous_state, &contract_result.final_state)?,
                DigestIdentifier::from_serializable_borsh(
                    serde_json::from_str::<Value>(&contract_result.final_state)
                        .unwrap()
                        .to_string(),
                )
                .map_err(|_| ExecutorErrorResponses::StateHashGenerationFailed)?,
            ),
            Acceptance::Ko => (String::from(""), DigestIdentifier::default()),
            Acceptance::Error => unreachable!(),
        };
        Ok(ExecuteContractResponse {
            json_patch: patch,
            hash_new_state: hash,
            governance_version,
            context_hash,
            success: contract_result.success,
            approval_required: contract_result.approval_required,
        })
    }
}

fn generate_json_patch(
    prev_state: &str,
    new_state: &str,
) -> Result<String, ExecutorErrorResponses> {
    let prev_state: Value = serde_json::from_str(prev_state)
        .map_err(|_| ExecutorErrorResponses::StateJSONDeserializationFailed)?;
    let new_state: Value = serde_json::from_str(new_state)
        .map_err(|_| ExecutorErrorResponses::StateJSONDeserializationFailed)?;
    let patch = diff(&prev_state, &new_state);
    Ok(serde_json::to_string(&patch)
        .map_err(|_| ExecutorErrorResponses::JSONPATCHDeserializationFailed)?)
}

fn generera_state_hash(state: &str) -> Result<DigestIdentifier, ExecutorErrorResponses> {
    DigestIdentifier::from_serializable_borsh(state)
        .map_err(|_| ExecutorErrorResponses::StateHashGenerationFailed)
}
