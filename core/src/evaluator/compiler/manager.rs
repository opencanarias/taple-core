use wasmtime::Engine;

use crate::{
    commons::channel::{ChannelData, MpscChannel},
    database::DB,
    evaluator::errors::{CompilerError, EvaluatorError, ExecutorErrorResponses},
    governance::GovernanceAPI,
    identifier::Derivable,
    DatabaseManager,
};

use crate::database::Error as DbError;

use super::{compiler::Compiler, CompilerMessages, CompilerResponses};

pub struct TapleCompiler<D: DatabaseManager> {
    input_channel: MpscChannel<CompilerMessages, CompilerResponses>,
    inner_compiler: Compiler<D>,
}

#[derive(Clone, Debug)]
enum CompilerCodes {
    MustShutdown,
    Ok,
}

impl<D: DatabaseManager> TapleCompiler<D> {
    pub fn new(
        input_channel: MpscChannel<CompilerMessages, CompilerResponses>,
        database: DB<D>,
        gov_api: GovernanceAPI,
        engine: Engine,
        contracts_path: String,
    ) -> Self {
        Self {
            input_channel,
            inner_compiler: Compiler::<D>::new(database, gov_api, engine, contracts_path),
        }
    }

    pub async fn start(mut self) {
        loop {
            let command = self.input_channel.receive().await;
            match command {
                Some(command) => {
                    let result = self.process_command(command).await;
                    if result.is_err() {
                        match result.unwrap_err() {
                            CompilerError::DatabaseError(_) => return,
                            CompilerError::ChannelNotAvailable => return,
                            CompilerError::InternalError(internal_error) => match internal_error {
                                crate::evaluator::errors::CompilerErrorResponses::DatabaseError(_) => return,
                                crate::evaluator::errors::CompilerErrorResponses::BorshSerializeContractError => return,
                                crate::evaluator::errors::CompilerErrorResponses::WriteFileError |
                                crate::evaluator::errors::CompilerErrorResponses::CargoExecError |
                                crate::evaluator::errors::CompilerErrorResponses::GarbageCollectorFail |
                                crate::evaluator::errors::CompilerErrorResponses::AddContractFail => todo!(),
                                crate::evaluator::errors::CompilerErrorResponses::GovernanceError(_) => return,
                            },
                        }
                    }
                    if let CompilerCodes::MustShutdown = result.unwrap() {
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
        command: ChannelData<CompilerMessages, CompilerResponses>,
    ) -> Result<CompilerCodes, CompilerError> {
        let (sender, data) = match command {
            ChannelData::AskData(data) => {
                let (sender, data) = data.get();
                (Some(sender), data)
            }
            ChannelData::TellData(data) => {
                let data = data.get();
                (None, data)
            }
        };

        let response = match data {
            CompilerMessages::NewGovVersion(data) => {
                CompilerResponses::CompileContract(self.inner_compiler.update_contracts(data).await)
            }
        };
        let Ok(_) = sender.unwrap().send(response) else {
            return Err(CompilerError::ChannelNotAvailable)
        };
        Ok(CompilerCodes::Ok)
    }
}
