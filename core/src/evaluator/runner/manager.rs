use json_patch::diff;

use crate::{
    commons::channel::{ChannelData, MpscChannel},
    database::DB,
    evaluator::errors::{EvaluatorError, ExecutorErrorResponses},
    identifier::Derivable,
    DatabaseManager,
};

use super::{executor::ContractExecutor, RunnerMessages, RunnerResponses};
use crate::database::Error as DbError;
pub struct TapleRunner<D: DatabaseManager> {
    input_channel: MpscChannel<RunnerMessages, RunnerResponses>,
    database: DB<D>,
    executor: ContractExecutor,
}

enum ExecutorCodes {
    MustShutdown,
    Ok,
}

impl<D: DatabaseManager> TapleRunner<D> {
    pub fn new(
        input_channel: MpscChannel<RunnerMessages, RunnerResponses>,
        database: DB<D>,
    ) -> Self {
        Self {
            input_channel,
            database,
            executor: ContractExecutor::new(),
        }
    }

    pub async fn start(mut self) {
        loop {
            let command = self.input_channel.receive().await;
            match command {
                Some(command) => {
                    let result = self.process_command(command).await;
                    if result.is_err() {
                        // TODO: Decidir si este o el componente padre el que decide si apagar el módulo
                        return;
                    }
                    if let ExecutorCodes::MustShutdown = result.unwrap() {
                        return;
                    }
                }
                None => {
                    return;
                }
            }
        }
    }

    async fn process_command(
        &mut self,
        command: ChannelData<RunnerMessages, RunnerResponses>,
    ) -> Result<ExecutorCodes, EvaluatorError> {
        let (sender, data) = match command {
            ChannelData::AskData(data) => {
                let (sender, data) = data.get();
                (Some(sender), data)
            }
            ChannelData::TellData(data) => {
                return Err(EvaluatorError::TellNotAvailable);
            }
        };

        let response = 'response: {
            match data {
                RunnerMessages::ExecuteContract(execute_contract) => {
                    // Read the contract from database
                    let contract = match self
                        .database
                        .get_contract(&execute_contract.governance_id, &execute_contract.schema)
                    {
                        Ok((contract, _)) => contract,
                        Err(DbError::EntryNotFound) => {
                            // Añadir en la response
                            break 'response RunnerResponses::ExecuteContract(Err(
                                ExecutorErrorResponses::ContractNotFound(
                                    execute_contract.schema,
                                    execute_contract.governance_id.to_str(),
                                ),
                            ));
                        }
                        Err(error) => return Err(EvaluatorError::DatabaseError(error.to_string())),
                    };
                    let previous_state = execute_contract.state.clone();
                    let contract_result = match self.executor.execute_contract(
                        execute_contract.state,
                        execute_contract.event,
                        contract,
                    ) {
                        Ok(contract_result) => contract_result,
                        Err(error) => break 'response RunnerResponses::ExecuteContract(Err(error)),
                    };
                    match generate_json_patch(previous_state, contract_result) {
                        Ok(patch) => {
                            RunnerResponses::ExecuteContract(Ok(super::ExecuteContractResponse {
                                json_patch: patch,
                            }))
                        }
                        Err(error) => RunnerResponses::ExecuteContract(Err(error)),
                    }
                }
                RunnerMessages::Shutdown => {
                    return Ok(ExecutorCodes::MustShutdown);
                }
            }
        };
        let Ok(_) = sender.unwrap().send(response) else {
          return Err(EvaluatorError::ChannelNotAvailable)
        };
        Ok(ExecutorCodes::Ok)
    }
}

fn generate_json_patch(
    prev_state: String,
    new_state: String,
) -> Result<String, ExecutorErrorResponses> {
    let prev_state = serde_json::to_value(prev_state)
        .map_err(|_| ExecutorErrorResponses::StateJSONDeserializationFailed)?;
    let new_state = serde_json::to_value(new_state)
        .map_err(|_| ExecutorErrorResponses::StateJSONDeserializationFailed)?;
    let patch = diff(&prev_state, &new_state);
    Ok(serde_json::to_string(&patch)
        .map_err(|_| ExecutorErrorResponses::JSONPATCHDeserializationFailed)?)
}
