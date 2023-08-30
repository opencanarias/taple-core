use json_patch::diff;
use serde_json::Value;
use wasmtime::Engine;

use crate::{
    commons::{
        models::{evaluation::EvaluationRequest, HashId},
        schema_handler::Schema,
    },
    database::DB,
    evaluator::errors::ExecutorErrorResponses,
    governance::GovernanceInterface,
    identifier::DigestIdentifier,
    request::FactRequest,
    DatabaseCollection, Derivable, EvaluationResponse, EventRequest, ValueWrapper,
};

use super::executor::{ContractExecutor, ContractResult};
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
        execute_contract: &EvaluationRequest,
    ) -> Result<DigestIdentifier, ExecutorErrorResponses> {
        DigestIdentifier::from_serializable_borsh(execute_contract)
            .map_err(|_| ExecutorErrorResponses::ContextHashGenerationFailed)
    }

    pub async fn execute_contract(
        &self,
        execute_contract: &EvaluationRequest,
        state_data: &FactRequest,
    ) -> Result<EvaluationResponse, ExecutorErrorResponses> {
        // Check governance version
        let governance_id = if &execute_contract.context.schema_id == "governance" {
            if let EventRequest::Fact(data) = &execute_contract.event_request.content {
                data.subject_id.clone()
            } else {
                return Err(ExecutorErrorResponses::CreateRequestNotAllowed);
            }
        } else {
            execute_contract.context.governance_id.clone()
        };
        let context_hash = Self::generate_context_hash(execute_contract)?;
        let governance = match self.database.get_subject(&governance_id) {
            Ok(governance) => governance,
            Err(DbError::EntryNotFound) => {
                // Revisar esto
                return Err(ExecutorErrorResponses::GovernanceNotFound);
            }
            Err(error) => return Err(ExecutorErrorResponses::DatabaseError(error.to_string())),
        };
        if governance.sn > execute_contract.gov_version {
            // Nuestra gov es mayor: mandamos mensaje para que actualice el emisor
            return Err(ExecutorErrorResponses::OurGovIsHigher);
        } else if governance.sn < execute_contract.gov_version {
            // Nuestra gov es menor: no podemos hacer nada. Pedimos LCE al que nos lo envió
            return Err(ExecutorErrorResponses::OurGovIsLower);
        }
        let (contract, contract_gov_version): (Vec<u8>, u64) = if execute_contract.context.schema_id
            == "governance"
            && execute_contract.context.governance_id.digest.is_empty()
        {
            match self.database.get_governance_contract() {
                Ok(contract) => (contract, governance.sn),
                Err(DbError::EntryNotFound) => {
                    return Err(ExecutorErrorResponses::ContractNotFound(
                        execute_contract.context.schema_id.clone(),
                        execute_contract.context.governance_id.to_str(),
                    ));
                }
                Err(error) => return Err(ExecutorErrorResponses::DatabaseError(error.to_string())),
            }
        } else {
            match self.database.get_contract(
                &execute_contract.context.governance_id,
                &execute_contract.context.schema_id,
            ) {
                Ok((contract, _, contract_gov_version)) => (contract, contract_gov_version),
                Err(DbError::EntryNotFound) => {
                    return Err(ExecutorErrorResponses::ContractNotFound(
                        execute_contract.context.schema_id.clone(),
                        execute_contract.context.governance_id.to_str(),
                    ));
                }
                Err(error) => return Err(ExecutorErrorResponses::DatabaseError(error.to_string())),
            }
        };
        if contract_gov_version != execute_contract.gov_version {
            return Err(ExecutorErrorResponses::ContractNotUpdated);
        }
        let previous_state = &execute_contract.context.state.clone();
        let mut contract_result = match self
            .executor
            .execute_contract(
                &execute_contract.context.state,
                &state_data.payload,
                contract,
                execute_contract.context.is_owner,
            )
            .await
        {
            Ok(contract_result) => contract_result,
            Err(error) => {
                match error {
                    ExecutorErrorResponses::ContractExecutionFailed
                    | ExecutorErrorResponses::ContractNotInstantiated
                    | ExecutorErrorResponses::ContractNotFound(_, _)
                    | ExecutorErrorResponses::ContractEntryPointNotFound
                    | ExecutorErrorResponses::FunctionLinkingFailed(_)
                    | ExecutorErrorResponses::SubjectError(_)
                    | ExecutorErrorResponses::CantGenerateContractResult
                    | ExecutorErrorResponses::StateHashGenerationFailed
                    | ExecutorErrorResponses::ContextHashGenerationFailed
                    | ExecutorErrorResponses::RolesObtentionFailed
                    | ExecutorErrorResponses::OurGovIsLower
                    | ExecutorErrorResponses::OurGovIsHigher
                    | ExecutorErrorResponses::CreateRequestNotAllowed
                    | ExecutorErrorResponses::GovernanceError(_)
                    | ExecutorErrorResponses::SchemaCompilationFailed
                    | ExecutorErrorResponses::InvalidPointerPovided => {
                        return Ok(EvaluationResponse {
                            patch: ValueWrapper(serde_json::from_str("[]").map_err(|_| {
                                ExecutorErrorResponses::JSONPATCHDeserializationFailed
                            })?),
                            state_hash: DigestIdentifier::from_serializable_borsh(
                                &execute_contract.context.state,
                            )
                            .map_err(|_| ExecutorErrorResponses::StateHashGenerationFailed)?,
                            eval_req_hash: context_hash,
                            eval_success: false,
                            appr_required: false,
                        })
                    }
                    _ => return Err(error),
                    // ExecutorErrorResponses::ValueToStringConversionFailed => todo!(),
                    //  ExecutorErrorResponses::StateJSONDeserializationFailed => todo!(),
                    //  ExecutorErrorResponses::JSONPATCHDeserializationFailed => todo!(),
                    // ExecutorErrorResponses::BorshSerializationError => todo!(),
                    // ExecutorErrorResponses::BorshDeserializationError => todo!(),
                    // ExecutorErrorResponses::DatabaseError(_) => return Err(error),
                }
            }
        };
        let (patch, hash) = match contract_result.success {
            true => {
                match self
                    .validation_state(
                        &contract_result,
                        &governance_id,
                        execute_contract.context.schema_id.clone(),
                        execute_contract.gov_version,
                    )
                    .await
                {
                    Ok(false) | Err(_) => {
                        contract_result.success = false;
                        contract_result.approval_required = false;
                        (
                            serde_json::from_str("[]").map_err(|_| {
                                ExecutorErrorResponses::JSONPATCHDeserializationFailed
                            })?,
                            execute_contract.context.state.hash_id()?,
                        )
                    }
                    _ => (
                        generate_json_patch(&previous_state.0, &contract_result.final_state.0)?,
                        DigestIdentifier::from_serializable_borsh(&contract_result.final_state)
                            .map_err(|_| ExecutorErrorResponses::StateHashGenerationFailed)?,
                    ),
                }
            }
            false => (
                serde_json::from_str("[]")
                    .map_err(|_| ExecutorErrorResponses::JSONPATCHDeserializationFailed)?,
                execute_contract.context.state.hash_id()?,
            ),
        };
        Ok(EvaluationResponse {
            patch: ValueWrapper(patch),
            state_hash: hash,
            eval_req_hash: context_hash,
            eval_success: contract_result.success,
            appr_required: contract_result.approval_required,
        })
    }

    async fn validation_state(
        &self,
        contract_result: &ContractResult,
        governance_id: &DigestIdentifier,
        schema_id: String,
        governance_version: u64,
    ) -> Result<bool, ExecutorErrorResponses> {
        if contract_result.success {
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
