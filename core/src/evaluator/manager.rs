use std::sync::Arc;

use wasmtime::Engine;

use super::compiler::manager::TapleCompiler;
use super::compiler::{CompilerMessages, CompilerResponses};
use super::errors::EvaluatorError;
use super::runner::ExecuteContract;
use super::{EvaluatorMessage, EvaluatorResponse};
use crate::database::{DatabaseManager, DB};
use crate::evaluator::errors::ExecutorErrorResponses;
use crate::evaluator::runner::manager::TapleRunner;
use crate::evaluator::AskForEvaluationResponse;
use crate::event_request::{EventRequestType, RequestPayload};
use crate::governance::GovernanceInterface;
use crate::protocol::command_head_manager::self_signature_manager::SelfSignatureInterface;
use crate::{
    commons::channel::{ChannelData, MpscChannel, SenderEnd},
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

pub struct EvaluatorManager<D: DatabaseManager + Send + 'static> {
    /// Communication channel for incoming petitions
    input_channel: MpscChannel<EvaluatorMessage, EvaluatorResponse>,
    /// Contract executioner
    runner: TapleRunner<D>,
    signature_manager: SelfSignatureManager,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl<D: DatabaseManager> EvaluatorManager<D> {
    pub fn new<G: GovernanceInterface + Send + 'static>(
        input_channel: MpscChannel<EvaluatorMessage, EvaluatorResponse>,
        database: Arc<D>,
        signature_manager: SelfSignatureManager,
        compiler_channel: MpscChannel<CompilerMessages, CompilerResponses>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        gov_api: G,
        contracts_path: String,
    ) -> Self {
        let engine = Engine::default();
        let compiler = TapleCompiler::new(
            compiler_channel,
            DB::new(database.clone()),
            gov_api,
            contracts_path,
            engine.clone(),
            shutdown_sender.subscribe(),
        );
        tokio::spawn(async move {
            compiler.start().await;
        });
        Self {
            input_channel,
            runner: TapleRunner::new(DB::new(database.clone()), engine),
            signature_manager,
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
                            let governance_version = executor_response.governance_version;
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

#[cfg(test)]
mod test {
    use std::{collections::HashSet, str::FromStr, sync::Arc};

    use async_trait::async_trait;
    use json_patch::diff;
    use serde::{Deserialize, Serialize};

    use crate::{
        commons::{
            channel::{MpscChannel, SenderEnd},
            crypto::{Ed25519KeyPair, KeyGenerator, KeyMaterial, KeyPair},
        },
        evaluator::{
            compiler::{CompilerMessages, CompilerResponses, ContractType, NewGovVersion},
            errors::{EvaluatorErrorResponses, ExecutorErrorResponses, CompilerErrorResponses},
            EvaluatorMessage, EvaluatorResponse,
        },
        event_content::Metadata,
        event_request::{EventRequest, EventRequestType, RequestPayload, StateRequest},
        governance::{error::RequestError, GovernanceInterface, RequestQuorum},
        identifier::{DigestIdentifier, KeyIdentifier},
        protocol::command_head_manager::self_signature_manager::{
            SelfSignatureInterface, SelfSignatureManager,
        },
        ApprovalResponse, Event, MemoryManager, TimeStamp,
    };

    use crate::evaluator::manager::EvaluatorManager;

    // Event Family
    #[derive(Serialize, Deserialize, Debug)]
    pub enum EventType {
        Notify {
            chunk: Vec<u8>,
        },
        ModOne {
            data: u32,
            chunk: Vec<u8>,
        },
        ModTwo {
            data: u32,
            chunk: Vec<u8>,
        },
        ModThree {
            data: u32,
            chunk: Vec<u8>,
        },
        ModAll {
            data: (u32, u32, u32),
            chunk: Vec<u8>,
        },
    }

    // Subject State
    #[derive(Serialize, Deserialize, Debug)]
    pub struct Data {
        pub one: u32,
        pub two: u32,
        pub three: u32,
    }

    struct GovernanceMockup {}

    fn get_file_wrong() -> String {
        String::from(
            r#"
        #[no_mangle]
        pub unsafe fn main_function(state_ptr: i32, event_ptr: i32) {
        }
      "#,
        )
    }

    fn get_file_wrong2() -> String {
        String::from(
            r#"
        #[no_mangle]
        pub unsafe fn main_function(state_ptr: i32, event_ptr: i32) -> i32 {
            4
        }
      "#,
        )
    }

    fn get_file() -> String {
        String::from(
            r#"
        mod externf;
        mod sdk;
        use serde::{Deserialize, Serialize};
    
        // Intento de simulación de cómo podría ser un contrato
    
        // Definir "estado del sujeto"
        #[repr(C)]
        #[derive(Serialize, Deserialize)]
        pub struct Data {
            pub one: u32,
            pub two: u32,
            pub three: u32,
        }
    
        // Definir "Familia de eventos"
        #[derive(Serialize, Deserialize, Debug)]
        pub enum EventType {
            Notify,
            ModOne{data: u32},
            ModTwo{data: u32},
            ModThree{data: u32},
            ModAll{data: (u32, u32, u32)},
        }
    
        #[no_mangle]
        pub unsafe fn main_function(state_ptr: i32, event_ptr: i32) -> u32 {
            sdk::execute_contract(state_ptr, event_ptr, contract_logic)
        }
    
        // Lógica del contrato con los tipos de datos esperados
        // Devuelve el puntero a los datos escritos con el estado modificado
        fn contract_logic(state: &mut Data, event: &EventType) {
            // Sería posible añadir gestión de errores
            // Podría ser interesante hacer las operaciones directamente como serde_json:Value en lugar de "Custom Data"
            match event {
                EventType::ModAll{data} => {
                    // Evento que modifica el estado entero
                    state.one = data.0;
                    state.two = data.1;
                    state.three = data.2;
                }
                EventType::ModOne{data} => {
                    // Evento que modifica Data.one
                    state.one = *data;
                }
                EventType::ModTwo{data} => {
                    // Evento que modifica Data.two
                    state.two = *data;
                }
                EventType::ModThree{data} => {
                    // Evento que modifica Data.three
                    state.three = *data;
                }
                EventType::Notify => {
                    // Evento que no modifica el estado
                    // Estos eventos se añadirían a la cadena, pero dentro del contrato apenas harían algo
                }
            }
        } 
      "#,
        )
    }

    #[async_trait]
    impl GovernanceInterface for GovernanceMockup {
        async fn check_quorum(
            &self,
            _event: Event,
            _signers: &HashSet<KeyIdentifier>,
        ) -> Result<(bool, HashSet<KeyIdentifier>), RequestError> {
            unimplemented!()
        }
        async fn check_quorum_request(
            &self,
            _event_request: EventRequest,
            _approvals: HashSet<ApprovalResponse>,
        ) -> Result<(RequestQuorum, HashSet<KeyIdentifier>), RequestError> {
            unimplemented!()
        }
        async fn check_policy(
            &self,
            _governance_id: &DigestIdentifier,
            _governance_version: u64,
            _schema_id: &String,
            _subject_namespace: &String,
            _controller_namespace: &String,
        ) -> Result<bool, RequestError> {
            unimplemented!()
        }
        async fn get_validators(
            &self,
            _event: Event,
        ) -> Result<HashSet<KeyIdentifier>, RequestError> {
            unimplemented!()
        }
        async fn get_approvers(
            &self,
            _event_request: EventRequest,
        ) -> Result<HashSet<KeyIdentifier>, RequestError> {
            unimplemented!()
        }
        async fn get_governance_version(
            &self,
            _governance_id: &DigestIdentifier,
        ) -> Result<u64, RequestError> {
            unimplemented!()
        }
        async fn get_schema(
            &self,
            _governance_id: &DigestIdentifier,
            _schema_id: &String,
        ) -> Result<serde_json::Value, RequestError> {
            unimplemented!()
        }
        async fn is_governance(
            &self,
            _subject_id: &DigestIdentifier,
        ) -> Result<bool, RequestError> {
            unimplemented!()
        }
        async fn check_invokation_permission(
            &self,
            _subject_id: DigestIdentifier,
            _invokator: KeyIdentifier,
            _additional_payload: Option<String>,
            _metadata: Option<Metadata>,
        ) -> Result<(bool, bool), RequestError> {
            unimplemented!()
        }
        async fn get_contracts(
            &self,
            governance_id: DigestIdentifier,
        ) -> Result<Vec<(String, ContractType)>, RequestError> {
            if governance_id
                == DigestIdentifier::from_str("Jg2Nuv5bNs4swQGcPQ1CXs9MtcfwMVoeQDR2Ea1YNYJw")
                    .unwrap()
            {
                Ok(vec![(
                    "test".to_owned(),
                    ContractType::String(String::from("test")),
                )])
            } else if governance_id
            == DigestIdentifier::from_str("Jg2Nuc5bNs4swQGcPQ1CXs9MtcfwMVoeQDR2Ea1YNYJw")
                .unwrap() {
                    Ok(vec![("test".to_owned(), ContractType::String(get_file_wrong()))])
                } else if governance_id
                == DigestIdentifier::from_str("Jg2Nuc5bNs4swQGcPQ2CXs9MtcfwMVoeQDR2Ea2YNYJw")
                    .unwrap() {
                        Ok(vec![("test".to_owned(), ContractType::String(get_file_wrong2()))])
                    } else {
                Ok(vec![("test".to_owned(), ContractType::String(get_file()))])
            }
        }
    }

    fn build_module() -> (
        EvaluatorManager<MemoryManager>,
        SenderEnd<EvaluatorMessage, EvaluatorResponse>,
        SenderEnd<CompilerMessages, CompilerResponses>,
        SelfSignatureManager,
    ) {
        let (rx, sx) = MpscChannel::new(100);
        let (rx_compiler, sx_compiler) = MpscChannel::new(100);
        let database = Arc::new(MemoryManager::new());
        let keypair = KeyPair::Ed25519(Ed25519KeyPair::from_seed(&[]));
        let pk = keypair.public_key_bytes();
        let signature_manager = SelfSignatureManager {
            keys: keypair,
            identifier: KeyIdentifier::new(crate::KeyDerivator::Ed25519, &pk),
            digest_derivator: crate::DigestDerivator::Blake3_256,
        };
        let (shutdown_sx, shutdown_rx) = tokio::sync::broadcast::channel(100);
        let governance = GovernanceMockup {};
        let manager = EvaluatorManager::new(
            rx,
            database,
            signature_manager.clone(),
            rx_compiler,
            shutdown_sx,
            shutdown_rx,
            governance,
            "../contract".into(),
        );
        (manager, sx, sx_compiler, signature_manager)
    }

    fn create_event_request(
        json: String,
        signature_manager: &SelfSignatureManager,
    ) -> EventRequest {
        let request = EventRequestType::State(StateRequest {
            subject_id: DigestIdentifier::from_str("JXtZRpNgBWVg9v5YG9AaTNfCpPd-rCTTKrFW9cV8-JKs")
                .unwrap(),
            payload: RequestPayload::Json(json),
        });
        let timestamp = TimeStamp::now();
        let signature = signature_manager.sign(&(&request, &timestamp)).unwrap();
        let event_request = EventRequest {
            request,
            timestamp,
            signature,
            approvals: HashSet::new(),
        };
        event_request
    }

    fn generate_json_patch(prev_state: &str, new_state: &str) -> String {
        let prev_state = serde_json::to_value(prev_state).unwrap();
        let new_state = serde_json::to_value(new_state).unwrap();
        let patch = diff(&prev_state, &new_state);
        serde_json::to_string(&patch).unwrap()
    }

    #[test]
    fn contract_execution() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (evaluator, sx_evaluator, sx_compiler, signature_manager) = build_module();
            let initial_state = Data {
                one: 10,
                two: 11,
                three: 13,
            };
            let initial_state_json = serde_json::to_string(&initial_state).unwrap();
            let event = EventType::ModTwo {
                data: 100,
                chunk: vec![123, 45, 20],
            };

            let handler = tokio::spawn(async move {
                evaluator.start().await;
            });

            sx_compiler
                .ask(CompilerMessages::NewGovVersion(NewGovVersion {
                    governance_id: DigestIdentifier::from_str(
                        "JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw",
                    )
                    .unwrap(),
                    governance_version: 0,
                }))
                .await
                .unwrap();

            let response = sx_evaluator
                .ask(EvaluatorMessage::AskForEvaluation(
                    crate::evaluator::AskForEvaluation {
                        governance_id: DigestIdentifier::from_str(
                            "JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw",
                        )
                        .unwrap(),
                        schema_id: "test".into(),
                        state: initial_state_json.clone(),
                        invokation: create_event_request(
                            serde_json::to_string(&event).unwrap(),
                            &signature_manager,
                        ),
                    },
                ))
                .await
                .unwrap();
            let EvaluatorResponse::AskForEvaluation(result) = response;
            assert!(result.is_ok());
            let result = result.unwrap();
            let new_state = Data {
                one: 10,
                two: 100,
                three: 13,
            };
            let new_state_json = &serde_json::to_string(&new_state).unwrap();
            let hash = DigestIdentifier::from_serializable_borsh(new_state_json).unwrap();
            assert_eq!(hash, result.hash_new_state);
            let patch = generate_json_patch(&initial_state_json, &new_state_json);
            assert_eq!(patch, result.json_patch);
            assert_eq!(result.governance_version, 0);
            let own_identifier = signature_manager.get_own_identifier();
            assert_eq!(result.signature.content.signer, own_identifier);
            handler.abort();
        });
    }

    #[test]
    fn contract_execution_fail() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (evaluator, sx_evaluator, sx_compiler, signature_manager) = build_module();
            let initial_state = Data {
                one: 10,
                two: 11,
                three: 13,
            };
            let initial_state_json = serde_json::to_string(&initial_state).unwrap();
            let event = String::from("hola");

            let handler = tokio::spawn(async move {
                evaluator.start().await;
            });

            sx_compiler
                .ask(CompilerMessages::NewGovVersion(NewGovVersion {
                    governance_id: DigestIdentifier::from_str(
                        "JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw",
                    )
                    .unwrap(),
                    governance_version: 0,
                }))
                .await
                .unwrap();

            let response = sx_evaluator
                .ask(EvaluatorMessage::AskForEvaluation(
                    crate::evaluator::AskForEvaluation {
                        governance_id: DigestIdentifier::from_str(
                            "JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw",
                        )
                        .unwrap(),
                        schema_id: "test".into(),
                        state: initial_state_json.clone(),
                        invokation: create_event_request(
                            serde_json::to_string(&event).unwrap(),
                            &signature_manager,
                        ),
                    },
                ))
                .await
                .unwrap();
            let EvaluatorResponse::AskForEvaluation(result) = response;
            assert!(result.is_err());
            let EvaluatorErrorResponses::ContractExecutionError(ExecutorErrorResponses::ContractExecutionFailed) = result.unwrap_err() else {
                panic!("Invalid response received");
            };
            handler.abort();
        });
    }

    #[test]
    fn contract_execution_fail2() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (evaluator, sx_evaluator, sx_compiler, signature_manager) = build_module();
            let initial_state = String::from("hola");
            let initial_state_json = serde_json::to_string(&initial_state).unwrap();
            let event = EventType::ModTwo {
                data: 100,
                chunk: vec![123, 45, 20],
            };

            let handler = tokio::spawn(async move {
                evaluator.start().await;
            });

            sx_compiler
                .ask(CompilerMessages::NewGovVersion(NewGovVersion {
                    governance_id: DigestIdentifier::from_str(
                        "JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw",
                    )
                    .unwrap(),
                    governance_version: 0,
                }))
                .await
                .unwrap();

            let response = sx_evaluator
                .ask(EvaluatorMessage::AskForEvaluation(
                    crate::evaluator::AskForEvaluation {
                        governance_id: DigestIdentifier::from_str(
                            "JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw",
                        )
                        .unwrap(),
                        schema_id: "test".into(),
                        state: initial_state_json.clone(),
                        invokation: create_event_request(
                            serde_json::to_string(&event).unwrap(),
                            &signature_manager,
                        ),
                    },
                ))
                .await
                .unwrap();
            let EvaluatorResponse::AskForEvaluation(result) = response;
            assert!(result.is_err());
            println!("{:?}", result);
            let EvaluatorErrorResponses::ContractExecutionError(ExecutorErrorResponses::ContractExecutionFailed) = result.unwrap_err() else {
                panic!("Invalid response received");
            };
            handler.abort();
        });
    }

    #[test]
    fn contract_execution_wrong_gov_id() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (evaluator, sx_evaluator, sx_compiler, signature_manager) = build_module();
            let initial_state = Data {
                one: 10,
                two: 11,
                three: 13,
            };
            let initial_state_json = serde_json::to_string(&initial_state).unwrap();
            let event = EventType::ModTwo {
                data: 100,
                chunk: vec![123, 45, 20],
            };

            let handler = tokio::spawn(async move {
                evaluator.start().await;
            });

            sx_compiler
                .ask(CompilerMessages::NewGovVersion(NewGovVersion {
                    governance_id: DigestIdentifier::from_str(
                        "JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw",
                    )
                    .unwrap(),
                    governance_version: 0,
                }))
                .await
                .unwrap();

            let response = sx_evaluator
                .ask(EvaluatorMessage::AskForEvaluation(
                    crate::evaluator::AskForEvaluation {
                        governance_id: DigestIdentifier::from_str(
                            "Jg2Nuv5bNs4swQGcPQ1CXs9MtcfwMVoeQDR2Ea1YNYJw",
                        )
                        .unwrap(),
                        schema_id: "teste".into(),
                        state: initial_state_json.clone(),
                        invokation: create_event_request(
                            serde_json::to_string(&event).unwrap(),
                            &signature_manager,
                        ),
                    },
                ))
                .await;
            let result = response.unwrap();
            let EvaluatorResponse::AskForEvaluation(result) = result else {
                panic!("Invalid response received");
            };
            assert!(result.is_err());
            let EvaluatorErrorResponses::ContractExecutionError(ExecutorErrorResponses::ContractNotFound(_,_)) = result.unwrap_err() else {
                panic!("Invalid response received");
            };
            handler.abort();
        });
    }

    #[test]
    fn contract_compilation_no_sdk() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (evaluator, sx_evaluator, sx_compiler, signature_manager) = build_module();
            let initial_state = Data {
                one: 10,
                two: 11,
                three: 13,
            };
            let initial_state_json = serde_json::to_string(&initial_state).unwrap();
            let event = EventType::ModTwo {
                data: 100,
                chunk: vec![123, 45, 20],
            };

            let handler = tokio::spawn(async move {
                evaluator.start().await;
            });

            let response = sx_compiler
                .ask(CompilerMessages::NewGovVersion(NewGovVersion {
                    governance_id: DigestIdentifier::from_str(
                        "Jg2Nuc5bNs4swQGcPQ2CXs9MtcfwMVoeQDR2Ea2YNYJw",
                    )
                    .unwrap(),
                    governance_version: 0,
                }))
                .await
                .unwrap();
            if let CompilerResponses::CompileContract(Err(CompilerErrorResponses::NoSDKFound)) = response {
                handler.abort();
            } else {
                assert!(false)
            };
        });
    }

    #[test]
    fn contract_execution_wrong_entrypoint() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (evaluator, sx_evaluator, sx_compiler, signature_manager) = build_module();
            let initial_state = Data {
                one: 10,
                two: 11,
                three: 13,
            };
            let initial_state_json = serde_json::to_string(&initial_state).unwrap();
            let event = EventType::ModTwo {
                data: 100,
                chunk: vec![123, 45, 20],
            };

            let handler = tokio::spawn(async move {
                evaluator.start().await;
            });

            let response = sx_compiler
                .ask(CompilerMessages::NewGovVersion(NewGovVersion {
                    governance_id: DigestIdentifier::from_str(
                        "Jg2Nuc5bNs4swQGcPQ1CXs9MtcfwMVoeQDR2Ea1YNYJw",
                    )
                    .unwrap(),
                    governance_version: 0,
                }))
                .await
                .unwrap();
            println!("{:?}", response);
            let response = sx_evaluator
                .ask(EvaluatorMessage::AskForEvaluation(
                    crate::evaluator::AskForEvaluation {
                        governance_id: DigestIdentifier::from_str(
                            "Jg2Nuc5bNs4swQGcPQ1CXs9MtcfwMVoeQDR2Ea1YNYJw",
                        )
                        .unwrap(),
                        schema_id: "test".into(),
                        state: initial_state_json.clone(),
                        invokation: create_event_request(
                            serde_json::to_string(&event).unwrap(),
                            &signature_manager,
                        ),
                    },
                ))
                .await;
            let result = response.unwrap();
            let EvaluatorResponse::AskForEvaluation(result) = result else {
                panic!("Invalid response received");
            };
            assert!(result.is_err());
            println!("{:?}", result);
            let EvaluatorErrorResponses::ContractExecutionError(ExecutorErrorResponses::ContractEntryPointNotFound) = result.unwrap_err() else {
                panic!("Invalid response received");
            };
            handler.abort();
        });
    }

    #[test]
    fn compilation_error() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (_evaluator, _sx_evaluator, sx_compiler, signature_manager) = build_module();

            let response = sx_compiler
                .ask(CompilerMessages::NewGovVersion(NewGovVersion {
                    governance_id: DigestIdentifier::from_str(
                        "Jg2Nuv5bNs4swQGcPQ1CXs9MtcfwMVoeQDR2Ea1YNYJw",
                    )
                    .unwrap(),
                    governance_version: 10,
                }))
                .await
                .unwrap();
            let CompilerResponses::CompileContract(result) = response else {
                panic!("Invalid response received");
            };
            assert!(result.is_err());
            let CompilerErrorResponses::CargoExecError = result.unwrap_err() else {
                panic!("Invalid response received");
            };
        });
    }
}
