use std::marker::PhantomData;
use std::sync::Arc;

use wasmtime::Engine;

use super::compiler::manager::TapleCompiler;
use super::errors::EvaluatorError;
use super::{EvaluatorMessage, EvaluatorResponse};
use crate::commons::channel::{ChannelData, MpscChannel, SenderEnd};
use crate::commons::self_signature_manager::{SelfSignatureInterface, SelfSignatureManager};
use crate::database::{DatabaseCollection, DatabaseManager, DB};
use crate::evaluator::errors::ExecutorErrorResponses;
use crate::evaluator::runner::manager::TapleRunner;
use crate::event_request::EventRequestType;
use crate::governance::{GovernanceInterface, GovernanceUpdatedMessage};
use crate::message::{MessageConfig, MessageTaskCommand};
use crate::protocol::protocol_message_manager::TapleMessages;
use crate::utils::message::event::create_evaluator_response;

pub struct EvaluatorManager<
    M: DatabaseManager<C>,
    C: DatabaseCollection + 'static,
> {
    /// Communication channel for incoming petitions
    input_channel: MpscChannel<EvaluatorMessage, EvaluatorResponse>,
    /// Contract executioner
    runner: TapleRunner<C>,
    signature_manager: SelfSignatureManager,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    messenger_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    _m: PhantomData<M>,
}

impl<
        M: DatabaseManager<C>,
        C: DatabaseCollection,
    > EvaluatorManager<M, C>
{
    pub fn new<G: GovernanceInterface + Send + Clone + 'static>(
        input_channel: MpscChannel<EvaluatorMessage, EvaluatorResponse>,
        database: Arc<M>,
        signature_manager: SelfSignatureManager,
        compiler_channel: tokio::sync::broadcast::Receiver<GovernanceUpdatedMessage>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        gov_api: G,
        contracts_path: String,
        messenger_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    ) -> Self {
        let engine = Engine::default();
        let compiler = TapleCompiler::new(
            compiler_channel,
            DB::new(database.clone()),
            gov_api.clone(),
            contracts_path,
            engine.clone(),
            shutdown_sender.subscribe(),
            shutdown_sender.clone(),
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
            messenger_channel,
            _m: PhantomData::default(),
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
                                log::error!("{}", result.unwrap_err());
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
            ChannelData::TellData(data) => {
                let data = data.get();
                (None, data)
            }
        };
        let response = 'response: {
            match data {
                EvaluatorMessage::AskForEvaluation(data) => {
                    let EventRequestType::State(state_data) = &data.event_request.request else {
                        break 'response EvaluatorResponse::AskForEvaluation(Err(super::errors::EvaluatorErrorResponses::CreateRequestNotAllowed));
                    };
                    let result = self.runner.execute_contract(&data, state_data).await;
                    log::warn!("Execution result: {:?}", result);
                    match result {
                        Ok(executor_response) => {
                            let governance_version = executor_response.governance_version;
                            let signature = self
                                .signature_manager
                                .sign(&(
                                    &executor_response.context_hash,
                                    &executor_response.hash_new_state,
                                    governance_version,
                                    &executor_response.success,
                                    &executor_response.approval_required,
                                ))
                                .map_err(|_| EvaluatorError::SignatureGenerationFailed)?;
                            let msg = create_evaluator_response(
                                executor_response.context_hash,
                                executor_response.hash_new_state,
                                governance_version,
                                executor_response.success,
                                executor_response.approval_required,
                                executor_response.json_patch,
                                signature,
                            );
                            self.messenger_channel
                                .tell(MessageTaskCommand::Request(
                                    None,
                                    msg,
                                    vec![data.context.owner],
                                    MessageConfig::direct_response(),
                                ))
                                .await
                                .map_err(|_| EvaluatorError::ChannelNotAvailable)?;
                            EvaluatorResponse::AskForEvaluation(Ok(()))
                        }
                        Err(ExecutorErrorResponses::OurGovIsHigher) => {
                            // Mandar mensaje de actualización pendiente
                            self.messenger_channel
                                .tell(MessageTaskCommand::Request(
                                    None,
                                    TapleMessages::EventMessage(
                                        crate::event::EventCommand::HigherGovernanceExpected {
                                            governance_id: data.context.governance_id,
                                            who_asked: self.signature_manager.get_own_identifier(),
                                        },
                                    ),
                                    vec![data.context.owner],
                                    MessageConfig::direct_response(),
                                ))
                                .await
                                .map_err(|_| EvaluatorError::ChannelNotAvailable)?;
                            EvaluatorResponse::AskForEvaluation(Ok(()))
                        }
                        Err(ExecutorErrorResponses::OurGovIsLower) => {
                            // No podemos evaluar porque nos la van a rechazar
                            // Pedir LCE al que nos mando la petición
                            self.messenger_channel
                                .tell(MessageTaskCommand::Request(
                                    None,
                                    TapleMessages::LedgerMessages(
                                        crate::ledger::LedgerCommand::GetLCE {
                                            who_asked: self.signature_manager.get_own_identifier(),
                                            subject_id: data.context.governance_id,
                                        },
                                    ),
                                    vec![data.context.owner],
                                    MessageConfig::direct_response(),
                                ))
                                .await
                                .map_err(|_| EvaluatorError::ChannelNotAvailable)?;
                            EvaluatorResponse::AskForEvaluation(Ok(()))
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
        if sender.is_some() {
            sender
                .unwrap()
                .send(response)
                .map_err(|_| EvaluatorError::ChannelNotAvailable)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {

    use std::{collections::HashSet, str::FromStr, sync::Arc};

    use async_trait::async_trait;
    use json_patch::diff;
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use tokio::{sync::broadcast::Sender};

    use crate::{
        commons::{
            channel::{ChannelData, MpscChannel, SenderEnd},
            crypto::{Ed25519KeyPair, KeyGenerator, KeyMaterial, KeyPair},
            models::{
                event_preevaluation::{Context, EventPreEvaluation},
                state::Subject,
            },
            schema_handler::gov_models::{Contract},
            self_signature_manager::{SelfSignatureInterface, SelfSignatureManager},
        },
        database::{MemoryCollection, DB},
        evaluator::{
            compiler::ContractType, EvaluatorMessage,
            EvaluatorResponse,
        },
        event_content::Metadata,
        event_request::{EventRequest, EventRequestType, StateRequest},
        governance::{
            error::RequestError, stage::ValidationStage, GovernanceInterface,
            GovernanceUpdatedMessage,
        },
        identifier::{DigestIdentifier, KeyIdentifier},
        message::MessageTaskCommand,
        protocol::protocol_message_manager::TapleMessages,
        MemoryManager,
        TimeStamp, event::EventCommand,
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

    #[derive(Clone)]
    struct GovernanceMockup {}

    fn get_file_wrong() -> String {
        String::from(
            r#"
        #[no_mangle]
        pub unsafe fn main_function(state_ptr: i32, event_ptr: i32, roles_ptr: i32) {
            
        }
        "#,
        )
    }

    fn get_file_wrong2() -> String {
        String::from(
            r#"
        #[no_mangle]
        pub unsafe fn main_function(state_ptr: i32, event_ptr: i32, roles_ptr: i32) -> i32 {
            4
        }
        "#,
        )
    }

    fn get_file() -> String {
        String::from(
            r#"
            mod sdk;
            use serde::{Deserialize, Serialize};
            
            // Intento de simulación de cómo podría ser un contrato
            
            // Definir "estado del sujeto"
            #[repr(C)]
            #[derive(Serialize, Deserialize, Clone)]
            pub struct Data {
                pub one: u32,
                pub two: u32,
                pub three: u32,
            }
            
            // Definir "Familia de eventos"
            #[derive(Serialize, Deserialize, Debug)]
            pub enum EventType {
                Notify,
                Patch { data: String },
                ModOne { data: u32 },
                ModTwo { data: u32 },
                ModThree { data: u32 },
                ModAll { data: (u32, u32, u32) },
            }
            
            #[no_mangle]
            pub unsafe fn main_function(state_ptr: i32, event_ptr: i32, roles_ptr: i32) -> u32 {
                sdk::execute_contract(state_ptr, event_ptr, roles_ptr, contract_logic)
            }
            
            /*
                context -> inmutable con estado inicial roles y evento
                result -> mutable success y approvalRequired, y estado final
                approvalRequired por defecto a false y siempre false si KO o error
            */
            
            // Lógica del contrato con los tipos de datos esperados
            // Devuelve el puntero a los datos escritos con el estado modificado
            fn contract_logic(
                context: &sdk::Context<Data, EventType>,
                contract_result: &mut sdk::ContractResult<Data>,
            ) {
                // Sería posible añadir gestión de errores
                // Podría ser interesante hacer las operaciones directamente como serde_json:Value en lugar de "Custom Data"
                let state = &mut contract_result.final_state;
                let roles = &context.roles;
                match &context.event {
                    EventType::ModAll { data } => {
                        // Evento que modifica el estado entero
                        state.one = data.0;
                        state.two = data.1;
                        state.three = data.2;
                    }
                    EventType::ModOne { data } => {
                        // Evento que modifica Data.one
                        if roles.contains(&"RolA".into()) {
                            state.one = *data;
                        }
                    }
                    EventType::ModTwo { data } => {
                        // Evento que modifica Data.two
                        state.two = *data;
                    }
                    EventType::ModThree { data } => {
                        // Evento que modifica Data.three
                        state.three = *data;
                    }
                    EventType::Notify => {
                        // Evento que no modifica el estado
                        // Estos eventos se añadirían a la cadena, pero dentro del contrato apenas harían algo
                    }
                    EventType::Patch { data } => {
                        // Se recibe un JSON PATCH
                        // Se aplica directamente al estado
                        let patched_state = sdk::apply_patch(&data, &context.initial_state).unwrap();
                        *state = patched_state;
                        // El usuario debería añadir una función que compruebe el estado del sujeto.
                    }
                }
                contract_result.success = true;
            }            
        "#,
        )
    }

    #[async_trait]
    impl GovernanceInterface for GovernanceMockup {
        async fn get_init_state(
            &self,
            _governance_id: DigestIdentifier,
            _schema_id: String,
            _governance_version: u64,
        ) -> Result<Value, RequestError> {
            unimplemented!()
        }

        async fn get_schema(
            &self,
            _governance_id: DigestIdentifier,
            _schema_id: String,
            _governance_version: u64,
        ) -> Result<serde_json::Value, RequestError> {
            unimplemented!()
        }

        async fn get_signers(
            &self,
            _metadata: Metadata,
            _stage: ValidationStage,
        ) -> Result<HashSet<KeyIdentifier>, RequestError> {
            unimplemented!()
        }

        async fn get_quorum(
            &self,
            _metadata: Metadata,
            _stage: ValidationStage,
        ) -> Result<u32, RequestError> {
            unimplemented!()
        }

        async fn get_invoke_info(
            &self,
            metadata: Metadata,
            stage: ValidationStage,
            invoker: KeyIdentifier,
        ) -> Result<bool, RequestError> {
            unreachable!()
        }

        async fn get_contracts(
            &self,
            governance_id: DigestIdentifier,
            _governance_version: u64,
        ) -> Result<Vec<Contract>, RequestError> {
            if governance_id
                == DigestIdentifier::from_str("Jg2Nuv5bNs4swQGcPQ1CXs9MtcfwMVoeQDR2Ea1YNYJw")
                    .unwrap()
            {
                Ok(vec![Contract {
                    name: "test".to_owned(),
                    content: ContractType::String(String::from("test"))
                        .to_string()
                        .unwrap(),
                }])
            } else if governance_id
                == DigestIdentifier::from_str("Jg2Nuc5bNs4swQGcPQ1CXs9MtcfwMVoeQDR2Ea1YNYJw")
                    .unwrap()
            {
                Ok(vec![Contract {
                    name: "test".to_owned(),
                    content: ContractType::String(get_file_wrong()).to_string().unwrap(),
                }])
            } else if governance_id
                == DigestIdentifier::from_str("Jg2Nuc5bNs4swQGcPQ2CXs9MtcfwMVoeQDR2Ea2YNYJw")
                    .unwrap()
            {
                Ok(vec![Contract {
                    name: "test".to_owned(),
                    content: ContractType::String(get_file_wrong2()).to_string().unwrap(),
                }])
            } else {
                Ok(vec![Contract {
                    name: "test".to_owned(),
                    content: ContractType::String(get_file()).to_string().unwrap(),
                }])
            }
        }

        async fn get_governance_version(
            &self,
            _governance_id: DigestIdentifier,
            _subject_id: DigestIdentifier,
        ) -> Result<u64, RequestError> {
            unimplemented!()
        }

        async fn is_governance(&self, _subject_id: DigestIdentifier) -> Result<bool, RequestError> {
            unimplemented!()
        }

        async fn governance_updated(
            &self,
            _governance_id: DigestIdentifier,
            _governance_version: u64,
        ) -> Result<(), RequestError> {
            Ok(())
        }
    }

    fn build_module() -> (
        EvaluatorManager<MemoryManager, MemoryCollection>,
        SenderEnd<EvaluatorMessage, EvaluatorResponse>,
        Sender<GovernanceUpdatedMessage>,
        SelfSignatureManager,
        MpscChannel<MessageTaskCommand<TapleMessages>, ()>,
    ) {
        let (rx, sx) = MpscChannel::new(100);
        let (msg_rx, msg_sx) = MpscChannel::new(100);
        let (sx_compiler, rx_compiler) = tokio::sync::broadcast::channel(100);
        let keypair = KeyPair::Ed25519(Ed25519KeyPair::from_seed(&[]));
        let pk = keypair.public_key_bytes();
        let signature_manager = SelfSignatureManager {
            keys: keypair,
            identifier: KeyIdentifier::new(crate::KeyDerivator::Ed25519, &pk),
            digest_derivator: crate::DigestDerivator::Blake3_256,
        };
        let (shutdown_sx, shutdown_rx) = tokio::sync::broadcast::channel(100);
        let governance = GovernanceMockup {};
        let collection = Arc::new(MemoryManager::new());
        let database = DB::new(collection.clone());
        database
            .set_subject(
                &DigestIdentifier::from_str("JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw")
                    .unwrap(),
                create_governance_test(),
            )
            .unwrap();
        let manager = EvaluatorManager::new(
            rx,
            collection,
            signature_manager.clone(),
            rx_compiler,
            shutdown_sx,
            shutdown_rx,
            governance,
            "../../contracts".into(),
            msg_sx,
        );
        (manager, sx, sx_compiler, signature_manager, msg_rx)
    }

    fn create_governance_test() -> Subject {
        let initial_state = Data {
            one: 10,
            two: 11,
            three: 13,
        };
        let initial_state_json = serde_json::to_string(&initial_state).unwrap();
        Subject {
            keys: None,
            subject_id: DigestIdentifier::from_str("JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw")
                .unwrap(),
            governance_id: DigestIdentifier::from_str("").unwrap(),
            sn: 0,
            public_key: KeyIdentifier::from_str("EF3E6fTSLrsEWzkD2tkB6QbJU9R7IOkunImqp0PB_ejg")
                .unwrap(),
            namespace: "namespace1".into(),
            schema_id: "test".into(),
            owner: KeyIdentifier::from_str("EF3E6fTSLrsEWzkD2tkB6QbJU9R7IOkunImqp0PB_ejg").unwrap(),
            creator: KeyIdentifier::from_str("EF3E6fTSLrsEWzkD2tkB6QbJU9R7IOkunImqp0PB_ejg")
                .unwrap(),
            properties: initial_state_json,
            active: true
        }
    }

    fn create_event_request(
        json: String,
        signature_manager: &SelfSignatureManager,
    ) -> EventRequest {
        let request = EventRequestType::State(StateRequest {
            subject_id: DigestIdentifier::from_str("JXtZRpNgBWVg9v5YG9AaTNfCpPd-rCTTKrFW9cV8-JKs")
                .unwrap(),
            invokation: json,
        });
        let timestamp = TimeStamp::now();
        let signature = signature_manager.sign(&(&request, &timestamp)).unwrap();
        let event_request = EventRequest {
            request,
            timestamp,
            signature,
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
            let (evaluator, sx_evaluator, sx_compiler, signature_manager, mut msg_rx) =
                build_module();
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
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await; // Pausa para compilar el contrato
            sx_compiler
                .send(GovernanceUpdatedMessage::GovernanceUpdated {
                    governance_id: DigestIdentifier::from_str(
                        "JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw",
                    )
                    .unwrap(),
                    governance_version: 0,
                })
                .unwrap();
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await; // Pausa para compilar el contrato
            let response = sx_evaluator
                .ask(EvaluatorMessage::AskForEvaluation(
                    EventPreEvaluation {
                        event_request: create_event_request(
                            serde_json::to_string(&event).unwrap(),
                            &signature_manager,
                        ),
                        context: Context {
                            governance_id: DigestIdentifier::from_str(
                                "JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw",
                            )
                            .unwrap(),
                            schema_id: "test".into(),
                            creator: KeyIdentifier::from_str(
                                "EF3E6fTSLrsEWzkD2tkB6QbJU9R7IOkunImqp0PB_ejg",
                            )
                            .unwrap(),
                            owner: KeyIdentifier::from_str(
                                "EF3E6fTSLrsEWzkD2tkB6QbJU9R7IOkunImqp0PB_ejg",
                            )
                            .unwrap(),
                            actual_state: initial_state_json.clone(),
                            namespace: "namespace1".into(),
                            governance_version: 0,
                        },
                        sn: 1,
                    },
                ))
                .await
                .unwrap();
            let EvaluatorResponse::AskForEvaluation(result) = response;
            assert!(result.is_ok());
            let message = if let ChannelData::TellData(data) = msg_rx.receive().await.unwrap() {
                if let MessageTaskCommand::Request(_, data, _, _) = data.get() {
                    data
                } else {
                    panic!("Unexpected 2");
                }
            } else {
                panic!("Unexpected");
            };
            let (evaluation, json_patch, signature) = if let TapleMessages::EventMessage(event) = message {
                match event {
                    EventCommand::EvaluatorResponse { evaluation, json_patch, signature } => {
                        (evaluation, json_patch, signature)
                    }
                    _ => {
                        panic!("Unexpected 4");
                    }
                }
            } else {
                panic!("Unexpected 3");
            };
            let new_state = Data {
                one: 10,
                two: 100,
                three: 13,
            };
            assert_eq!(evaluation.governance_version, 0);
            let new_state_json = &serde_json::to_string(&new_state).unwrap();
            // let hash = DigestIdentifier::from_serializable_borsh(new_state_json).unwrap();
            // assert_eq!(hash, evaluation.state_hash); // arreglar
            println!("{:#?}\n{:#?}", initial_state_json, new_state_json);
            let patch = generate_json_patch(&initial_state_json, &new_state_json);
            assert_eq!(patch, json_patch); // arreglar
            // let own_identifier = signature_manager.get_own_identifier();
            // assert_eq!(evaluation..signer, own_identifier); // arreglar
            handler.abort();
        });
    }

    /* 
    #[test]
    fn contract_execution_fail() {
        // Fail reason: Bad Event
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (evaluator, sx_evaluator, sx_compiler, signature_manager, msg_rx) = build_module();
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
                .send(GovernanceUpdatedMessage::GovernanceUpdated {
                    governance_id: DigestIdentifier::from_str(
                        "JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw",
                    )
                    .unwrap(),
                    governance_version: 0,
                })
                .unwrap();
            // sx_compiler
            //     .ask(CompilerMessages::NewGovVersion(NewGovVersion {
            //         governance_id: DigestIdentifier::from_str(
            //             "JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw",
            //         )
            //         .unwrap(),
            //         governance_version: 0,
            //     }))
            //     .await
            //     .unwrap();

            let response = sx_evaluator
                .ask(EvaluatorMessage::AskForEvaluation(EventPreEvaluation {
                    // invokation: create_event_request(
                    //     serde_json::to_string(&event).unwrap(),
                    //     &signature_manager,
                    // ),
                    // hash_request: DigestIdentifier::default().to_str(),
                    event_request: create_event_request(
                        serde_json::to_string(&event).unwrap(),
                        &signature_manager,
                    ),
                    context: Context {
                        governance_id: DigestIdentifier::from_str(
                            "JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw",
                        )
                        .unwrap(),
                        schema_id: "test".into(),
                        creator: KeyIdentifier::from_str(
                            "EF3E6fTSLrsEWzkD2tkB6QbJU9R7IOkunImqp0PB_ejg",
                        )
                        .unwrap(),
                        owner: KeyIdentifier::from_str(
                            "EF3E6fTSLrsEWzkD2tkB6QbJU9R7IOkunImqp0PB_ejg",
                        )
                        .unwrap(),
                        actual_state: initial_state_json.clone(),
                        namespace: "namespace1".into(),
                        governance_version: 0,
                    },
                    sn: 1,
                }))
                .await
                .unwrap();
            let EvaluatorResponse::AskForEvaluation(result) = response;
            assert!(result.is_ok());
            // let result = result.unwrap();
            // assert!(!result.success);
            handler.abort();
        });
    }

    #[test]
    fn contract_execution_fail2() {
        // Fail reason: Bad State
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
                        invokation: create_event_request(
                            serde_json::to_string(&event).unwrap(),
                            &signature_manager,
                        ),
                        // hash_request: DigestIdentifier::default().to_str(),
                        context: Context {
                            governance_id: DigestIdentifier::from_str(
                                "JGSPR6FL-vE7iZxWMd17o09qn7NeTqlcImDVWmijXczw",
                            )
                            .unwrap(),
                            schema_id: "test".into(),
                            invokator: KeyIdentifier::from_str(
                                "EF3E6fTSLrsEWzkD2tkB6QbJU9R7IOkunImqp0PB_ejg",
                            )
                            .unwrap(),
                            creator: KeyIdentifier::from_str(
                                "EF3E6fTSLrsEWzkD2tkB6QbJU9R7IOkunImqp0PB_ejg",
                            )
                            .unwrap(),
                            owner: KeyIdentifier::from_str(
                                "EF3E6fTSLrsEWzkD2tkB6QbJU9R7IOkunImqp0PB_ejg",
                            )
                            .unwrap(),
                            state: initial_state_json.clone(),
                            namespace: "namespace1".into(),
                        },
                        sn: 1,
                    },
                ))
                .await
                .unwrap();
            let EvaluatorResponse::AskForEvaluation(result) = response;
            assert!(result.is_ok());
            let result = result.unwrap();
            assert!(!result.success);
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
                        invokation: create_event_request(
                            serde_json::to_string(&event).unwrap(),
                            &signature_manager,
                        ),
                        // hash_request: DigestIdentifier::default().to_str(),
                        context: Context {
                            governance_id: DigestIdentifier::from_str(
                                "Jg2Nuv5bNs4swQGcPQ1CXs9MtcfwMVoeQDR2Ea1YNYJw",
                            )
                            .unwrap(),
                            schema_id: "test".into(),
                            invokator: KeyIdentifier::from_str(
                                "EF3E6fTSLrsEWzkD2tkB6QbJU9R7IOkunImqp0PB_ejg",
                            )
                            .unwrap(),
                            creator: KeyIdentifier::from_str(
                                "EF3E6fTSLrsEWzkD2tkB6QbJU9R7IOkunImqp0PB_ejg",
                            )
                            .unwrap(),
                            owner: KeyIdentifier::from_str(
                                "EF3E6fTSLrsEWzkD2tkB6QbJU9R7IOkunImqp0PB_ejg",
                            )
                            .unwrap(),
                            state: initial_state_json.clone(),
                            namespace: "namespace1".into(),
                        },
                        sn: 1,
                    },
                ))
                .await
                .unwrap();
            let EvaluatorResponse::AskForEvaluation(result) = response;
            assert!(result.is_ok());
            let result = result.unwrap();
            assert!(!result.success);
            handler.abort();
        });
    }

    #[test]
    fn contract_compilation_no_sdk() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (evaluator, _sx_evaluator, sx_compiler, signature_manager) = build_module();

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
            if let CompilerResponses::CompileContract(Err(CompilerErrorResponses::NoSDKFound)) =
                response
            {
                handler.abort();
            } else {
                assert!(false)
            };
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
    */
}
