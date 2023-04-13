use json_patch::diff;
use wasmtime::Engine;

use crate::{
    database::DB,
    evaluator::{errors::ExecutorErrorResponses, AskForEvaluation, Context},
    governance::GovernanceInterface,
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
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
        context: &Context,
        sn: u64,
    ) -> Result<DigestIdentifier, ExecutorErrorResponses> {
        DigestIdentifier::from_serializable_borsh((context, sn))
            .map_err(|_| ExecutorErrorResponses::ContextHashGenerationFailed)
    }

    pub async fn execute_contract(
        &self,
        execute_contract: AskForEvaluation,
    ) -> Result<ExecuteContractResponse, ExecutorErrorResponses> {
        let context_hash =
            Self::generate_context_hash(&execute_contract.context, execute_contract.sn)?;
        let (contract, governance_version) = match self.database.get_contract(
            &execute_contract.context.governance_id,
            &execute_contract.context.schema_id,
        ) {
            Ok((contract, _, governance_version)) => (contract, governance_version),
            Err(DbError::EntryNotFound) => {
                let governance_version = match self
                    .database
                    .get_subject(&execute_contract.context.governance_id)
                {
                    Ok(governance) => governance.subject_data.as_ref().unwrap().sn,
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
                    success: false,
                    approval_required: false,
                });
            }
            Err(error) => return Err(ExecutorErrorResponses::DatabaseError(error.to_string())),
        };
        let previous_state = execute_contract.state.clone();
        let contract_result = self
            .executor
            .execute_contract(
                execute_contract.state,
                execute_contract.data,
                execute_contract.context,
                governance_version,
                contract,
            )
            .await?;
        let (patch, hash) = if contract_result.success {
            (
                generate_json_patch(&previous_state, &contract_result.final_state)?,
                generera_state_hash(&contract_result.final_state)?,
            )
        } else {
            (String::from(""), DigestIdentifier::default())
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
    let prev_state = serde_json::to_value(prev_state)
        .map_err(|_| ExecutorErrorResponses::StateJSONDeserializationFailed)?;
    let new_state = serde_json::to_value(new_state)
        .map_err(|_| ExecutorErrorResponses::StateJSONDeserializationFailed)?;
    let patch = diff(&prev_state, &new_state);
    Ok(serde_json::to_string(&patch)
        .map_err(|_| ExecutorErrorResponses::JSONPATCHDeserializationFailed)?)
}

fn generera_state_hash(state: &str) -> Result<DigestIdentifier, ExecutorErrorResponses> {
    DigestIdentifier::from_serializable_borsh(state)
        .map_err(|_| ExecutorErrorResponses::StateHashGenerationFailed)
}
