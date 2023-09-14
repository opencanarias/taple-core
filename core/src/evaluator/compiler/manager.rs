use tokio_util::sync::CancellationToken;
use wasmtime::Engine;

use crate::{
    database::DB,
    evaluator::errors::CompilerError,
    governance::{GovernanceInterface, GovernanceUpdatedMessage},
    DatabaseCollection, Notification,
};

use super::compiler::Compiler;

pub struct TapleCompiler<C: DatabaseCollection, G: GovernanceInterface> {
    input_channel: tokio::sync::broadcast::Receiver<GovernanceUpdatedMessage>,
    inner_compiler: Compiler<C, G>,
    token: CancellationToken,
    notification_tx: tokio::sync::mpsc::Sender<Notification>,
}

impl<C: DatabaseCollection, G: GovernanceInterface + Send> TapleCompiler<C, G> {
    pub fn new(
        input_channel: tokio::sync::broadcast::Receiver<GovernanceUpdatedMessage>,
        database: DB<C>,
        gov_api: G,
        contracts_path: String,
        engine: Engine,
        token: CancellationToken,
        notification_tx: tokio::sync::mpsc::Sender<Notification>,
    ) -> Self {
        Self {
            input_channel,
            inner_compiler: Compiler::<C, G>::new(database, gov_api, engine, contracts_path),
            token,
            notification_tx,
        }
    }

    pub async fn start(mut self) {
        let init = self.inner_compiler.init().await;
        if let Err(error) = init {
            log::error!("Evaluator Compiler error: {}", error);
            self.token.cancel();
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
                _ = self.token.cancelled() => {
                    log::debug!("Compiler module shutdown received");
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
                let _result = self
                    .inner_compiler
                    .update_contracts(governance_id, governance_version)
                    .await;
            }
        };
        Ok(())
    }
}
