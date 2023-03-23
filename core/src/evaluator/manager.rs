use wasmtime::Engine;

use super::errors::EvaluatorError;
use super::runner::ExecuteContract;
use super::{EvaluatorMessage, EvaluatorResponse};
use crate::commons::crypto::KeyPair;
use crate::database::{DatabaseManager, DB};
use crate::evaluator::errors::ExecutorErrorResponses;
use crate::evaluator::runner::manager::TapleRunner;
use crate::evaluator::AskForEvaluationResponse;
use crate::event_request::{EventRequestType, RequestPayload};
use crate::protocol::command_head_manager::self_signature_manager::SelfSignatureInterface;
use crate::TapleSettings;
use crate::{
    commons::channel::{ChannelData, MpscChannel, SenderEnd},
    governance::GovernanceAPI,
    protocol::command_head_manager::self_signature_manager::SelfSignatureManager,
};

#[derive(Clone, Debug)]
pub struct EvaluatorAPI {
    sender: SenderEnd<EvaluatorMessage, EvaluatorResponse>,
}

impl EvaluatorAPI {
    pub fn new(sender: SenderEnd<EvaluatorMessage, EvaluatorResponse>) -> Self {
        Self { sender }
    }
}

pub struct EvaluatorManager<D: DatabaseManager> {
    /// Communication channel for incoming petitions
    input_channel: MpscChannel<EvaluatorMessage, EvaluatorResponse>,
    /// Contract executioned
    runner: TapleRunner<D>,
    signature_manager: SelfSignatureManager,
    // TODO: Añadir módulo compilación
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl<D: DatabaseManager> EvaluatorManager<D> {
    pub fn new(
        input_channel: MpscChannel<EvaluatorMessage, EvaluatorResponse>,
        gov_api: GovernanceAPI,
        database: DB<D>,
        settings: &TapleSettings,
        keys: &KeyPair,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    ) -> Self {
        let engine = Engine::default();
        Self {
            input_channel,
            runner: TapleRunner::new(database, engine),
            signature_manager: SelfSignatureManager::new(keys.clone(), settings),
            shutdown_receiver,
            shutdown_sender,
        }
    }

    pub async fn start(mut self) {
        loop {
            tokio::select! {
                command = self.input_channel.receive() => {
                    match command {
                        Some(command) => {
                            let result = self.process_command(command).await;
                            if result.is_err() {
                                self.shutdown_sender.send(()).expect("Channel Closed");
                            }
                        }
                        None => {
                            self.shutdown_sender.send(()).expect("Channel Closed");
                        },
                    }
                },
                _ = self.shutdown_receiver.recv() => {
                    break;
                }
            }
        }
    }

    async fn process_command(
        &mut self,
        command: ChannelData<EvaluatorMessage, EvaluatorResponse>,
    ) -> Result<(), EvaluatorError> {
        let (sender, data) = match command {
            ChannelData::AskData(data) => {
                let (sender, data) = data.get();
                (Some(sender), data)
            }
            ChannelData::TellData(_) => {
                return Err(EvaluatorError::TellNotAvailable);
            }
        };
        let response = 'response: {
            match data {
                EvaluatorMessage::AskForEvaluation(data) => {
                    let EventRequestType::State(state_data) = &data.invokation.request else {
                        break 'response EvaluatorResponse::AskForEvaluation(Err(super::errors::EvaluatorErrorResponses::CreateRequestNotAllowed));
                    };
                    let result = self
                        .runner
                        .execute_contract(ExecuteContract {
                            governance_id: data.governance_id,
                            schema: data.schema_id,
                            state: data.state,
                            event: extract_data_from_payload(&state_data.payload),
                        })
                        .await;
                    match result {
                        Ok(executor_response) => {
                            let governance_version = 0;
                            let signature = self
                                .signature_manager
                                .sign(&(
                                    &executor_response.hash_new_state,
                                    &executor_response.json_patch,
                                    governance_version,
                                ))
                                .map_err(|_| EvaluatorError::SignatureGenerationFailed)?;
                            EvaluatorResponse::AskForEvaluation(Ok(AskForEvaluationResponse {
                                governance_version,
                                hash_new_state: executor_response.hash_new_state,
                                json_patch: executor_response.json_patch,
                                signature,
                            }))
                        }
                        Err(ExecutorErrorResponses::DatabaseError(error)) => {
                            return Err(EvaluatorError::DatabaseError(error))
                        }
                        Err(
                            ExecutorErrorResponses::StateJSONDeserializationFailed
                            | ExecutorErrorResponses::JSONPATCHDeserializationFailed,
                        ) => return Err(EvaluatorError::JSONDeserializationFailed),
                        Err(error) => {
                            break 'response EvaluatorResponse::AskForEvaluation(Err(
                                super::errors::EvaluatorErrorResponses::ContractExecutionError(
                                    error,
                                ),
                            ))
                        }
                    }
                }
            }
        };
        sender
            .unwrap()
            .send(response)
            .map_err(|_| EvaluatorError::ChannelNotAvailable)?;
        Ok(())
    }
}

fn extract_data_from_payload(payload: &RequestPayload) -> String {
    match payload {
        RequestPayload::Json(data) => data.clone(),
        RequestPayload::JsonPatch(data) => data.clone(),
    }
}
