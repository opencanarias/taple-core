use wasmtime::Engine;

use crate::{
    database::DB,
    evaluator::errors::CompilerError,
    governance::{GovernanceInterface, GovernanceUpdatedMessage},
    DatabaseManager,
};

use super::compiler::Compiler;

pub struct TapleCompiler<D: DatabaseManager, G: GovernanceInterface> {
    input_channel: tokio::sync::broadcast::Receiver<GovernanceUpdatedMessage>,
    inner_compiler: Compiler<D, G>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
}

#[derive(Clone, Debug)]
enum CompilerCodes {
    MustShutdown,
    Ok,
}

impl<D: DatabaseManager, G: GovernanceInterface + Send> TapleCompiler<D, G> {
    pub fn new(
        input_channel: tokio::sync::broadcast::Receiver<GovernanceUpdatedMessage>,
        database: DB<D>,
        gov_api: G,
        contracts_path: String,
        engine: Engine,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
    ) -> Self {
        Self {
            input_channel,
            inner_compiler: Compiler::<D, G>::new(database, gov_api, engine, contracts_path),
            shutdown_receiver,
            shutdown_sender,
        }
    }

    pub async fn start(mut self) {
        let init = self.inner_compiler.init().await;
        if let Err(error) = init {
            log::error!("Evaluator Compiler error: {}", error);
            self.shutdown_sender.send(()).unwrap();
            return;
        }
        loop {
            tokio::select! {
                command = self.input_channel.recv() => {
                    match command {
                        Ok(command) => {
                            let result = self.process_command(command).await;
                            if result.is_err() {
                                match result.unwrap_err() {
                                    CompilerError::InitError(_) => unreachable!(),
                                    CompilerError::DatabaseError(_) => return,
                                    CompilerError::ChannelNotAvailable => return,
                                    CompilerError::InternalError(internal_error) => match internal_error {
                                        crate::evaluator::errors::CompilerErrorResponses::DatabaseError(_) => return,
                                        crate::evaluator::errors::CompilerErrorResponses::BorshSerializeContractError => return,
                                        crate::evaluator::errors::CompilerErrorResponses::WriteFileError |
                                        crate::evaluator::errors::CompilerErrorResponses::CargoExecError |
                                        crate::evaluator::errors::CompilerErrorResponses::GarbageCollectorFail |
                                        crate::evaluator::errors::CompilerErrorResponses::TempFolderCreationFailed |
                                        crate::evaluator::errors::CompilerErrorResponses::InvalidImportFound |
                                        crate::evaluator::errors::CompilerErrorResponses::NoSDKFound |
                                        crate::evaluator::errors::CompilerErrorResponses::AddContractFail => todo!(),
                                        crate::evaluator::errors::CompilerErrorResponses::GovernanceError(_) => return,
                                    },
                                }
                            }
                            if let CompilerCodes::MustShutdown = result.unwrap() {
                                return;
                            }
                        }
                        Err(_) => {
                            return;
                        }
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
        command: GovernanceUpdatedMessage,
    ) -> Result<CompilerCodes, CompilerError> {
        let response = match command {
            GovernanceUpdatedMessage::GovernanceUpdated {
                governance_id,
                governance_version,
            } => {
                self.inner_compiler
                    .update_contracts(governance_id, governance_version)
                    .await
            }
        };
        Ok(CompilerCodes::Ok)
    }
}
