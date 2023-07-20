use wasmtime::Engine;

use crate::{
    database::DB,
    evaluator::errors::CompilerError,
    governance::{GovernanceInterface, GovernanceUpdatedMessage},
    DatabaseCollection,
};

use super::compiler::Compiler;

pub struct TapleCompiler<C: DatabaseCollection, G: GovernanceInterface> {
    input_channel: tokio::sync::broadcast::Receiver<GovernanceUpdatedMessage>,
    inner_compiler: Compiler<C, G>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
}

impl<C: DatabaseCollection, G: GovernanceInterface + Send> TapleCompiler<C, G> {
    pub fn new(
        input_channel: tokio::sync::broadcast::Receiver<GovernanceUpdatedMessage>,
        database: DB<C>,
        gov_api: G,
        contracts_path: String,
        engine: Engine,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
    ) -> Self {
        Self {
            input_channel,
            inner_compiler: Compiler::<C, G>::new(database, gov_api, engine, contracts_path),
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
                                log::error!("Evaluator error: {}", result.unwrap_err())
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
    ) -> Result<(), CompilerError> {
        let _response = match command {
            GovernanceUpdatedMessage::GovernanceUpdated {
                governance_id,
                governance_version,
            } => {
                let result = self
                    .inner_compiler
                    .update_contracts(governance_id, governance_version)
                    .await;
            }
        };
        Ok(())
    }
}
