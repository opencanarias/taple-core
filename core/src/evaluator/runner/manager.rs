use json_patch::diff;
use wasmtime::Engine;

use crate::{
    database::DB,
    evaluator::errors::{ExecutorErrorResponses},
    identifier::{Derivable, DigestIdentifier},
    DatabaseManager,
};

use super::{
    executor::ContractExecutor, ExecuteContract, ExecuteContractResponse,
};
use crate::database::Error as DbError;
pub struct TapleRunner<D: DatabaseManager> {
    database: DB<D>,
    executor: ContractExecutor,
}

impl<D: DatabaseManager> TapleRunner<D> {

    pub fn new(
        database: DB<D>,
        engine: Engine,
    ) -> Self {
        Self {
            database,
            executor: ContractExecutor::new(engine),
        }
    }

    pub async fn execute_contract(
        &self,
        execute_contract: ExecuteContract,
    ) -> Result<ExecuteContractResponse, ExecutorErrorResponses> {
        let (contract, governance_version) = match self
            .database
            .get_contract(&execute_contract.governance_id, &execute_contract.schema)
        {
            Ok((contract, _, governance_version)) => (contract, governance_version),
            Err(DbError::EntryNotFound) => {
                return Err(ExecutorErrorResponses::ContractNotFound(
                    execute_contract.schema,
                    execute_contract.governance_id.to_str(),
                ));
            }
            Err(error) => return Err(ExecutorErrorResponses::DatabaseError(error.to_string())),
        };
        let previous_state = execute_contract.state.clone();
        let contract_result = self.executor.execute_contract(
            execute_contract.state,
            execute_contract.event,
            contract,
        )?;
        let patch = generate_json_patch(&previous_state, &contract_result)?;
        let hash = generera_state_hash(contract_result)?;
        Ok(ExecuteContractResponse {
            json_patch: patch,
            hash_new_state: hash,
            governance_version,
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

fn generera_state_hash(state: String) -> Result<DigestIdentifier, ExecutorErrorResponses> {
    DigestIdentifier::from_serializable_borsh(state)
        .map_err(|_| ExecutorErrorResponses::StateHashGenerationFailed)
}
