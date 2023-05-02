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
    DatabaseManager,
};

use super::{executor::ContractExecutor, ExecuteContractResponse};
use crate::database::Error as DbError;
pub struct TapleRunner<D: DatabaseManager, G: GovernanceInterface + Send> {
    database: DB<D>,
    executor: ContractExecutor<G>,
}

impl<D: DatabaseManager, G: GovernanceInterface + Send> TapleRunner<D, G> {
    pub fn new(database: DB<D>, engine: Engine, gov_api: G) -> Self {
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
        log::info!("PASA EVALUATOR - EXECEUTE CONTRACT");
        let context_hash = Self::generate_context_hash(execute_contract)?;
        log::info!("PASA EVALUATOR - GENERATE HASH");
        let (contract, governance_version) = if execute_contract.context.schema_id == "governance"
            && execute_contract.context.governance_id.digest.is_empty()
        {
            match self.database.get_governance_contract() {
                // TODO: Gestionar versión gobernanza
                Ok(contract) => (contract, execute_contract.context.governance_version),
                Err(DbError::EntryNotFound) => {
                    log::info!("PASA EVALUATOR - NOT FOUND");
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
                    log::info!("PASA EVALUATOR - NOT FOUND");
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
        // log::info!("PASA EVALUATOR - AFTER CONTRACT RESULT");
        // log::info!("PASA EVALUATOR - contract_result: {:?}", contract_result);
        // log::error!("INITIAL_STATE: {}", previous_state);
        log::error!(
            "contract_result.final_state: {}",
            contract_result.final_state
        );
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
        log::error!("PATCH: {}", patch);
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
    log::error!("prev state {}", prev_state);
    log::error!("is string {}", prev_state.is_string());
    let new_state: Value = serde_json::from_str(new_state)
        .map_err(|_| ExecutorErrorResponses::StateJSONDeserializationFailed)?;
    log::error!("end state {}", new_state);
    let patch = diff(&prev_state, &new_state);
    Ok(serde_json::to_string(&patch)
        .map_err(|_| ExecutorErrorResponses::JSONPATCHDeserializationFailed)?)
}

fn generera_state_hash(state: &str) -> Result<DigestIdentifier, ExecutorErrorResponses> {
    DigestIdentifier::from_serializable_borsh(state)
        .map_err(|_| ExecutorErrorResponses::StateHashGenerationFailed)
}
