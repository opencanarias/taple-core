use async_trait::async_trait;
use crate::commons::{
    bd::db::DB,
    channel::{AskData, ChannelData, MpscChannel, SenderEnd},
    identifier::{DigestIdentifier, KeyIdentifier},
    models::{
        event::Event,
        event_request::EventRequest,
        signature::Signature,
        state::{LedgerState, Subject, SubjectData},
    },
};
use crate::governance::{GovernanceAPI, GovernanceMessage, GovernanceResponse};
use std::collections::{HashMap, HashSet};

use super::super::errors::LedgerManagerError;

use super::{ledger::Ledger, CommandManagerMessage, CommandManagerResponse, EventSN};

#[async_trait]
pub trait LedgerInterface {
    async fn get_event(
        &self,
        subject_id: &DigestIdentifier,
        sn: EventSN,
    ) -> Result<(Event, LedgerState), LedgerManagerError>;
    async fn get_signatues(
        &self,
        subject_id: DigestIdentifier,
        sn: EventSN,
    ) -> Result<(HashSet<Signature>, LedgerState), LedgerManagerError>;
    async fn get_signers(
        &self,
        subject_id: DigestIdentifier,
        sn: EventSN,
    ) -> Result<(HashSet<KeyIdentifier>, LedgerState), LedgerManagerError>;
    async fn get_subject(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<SubjectData, LedgerManagerError>;
    async fn get_subjects(&self, namespace: String)
        -> Result<Vec<SubjectData>, LedgerManagerError>;
    async fn get_subjects_raw(&self, namespace: String)
        -> Result<Vec<Subject>, LedgerManagerError>;
    async fn put_event(&self, event: Event) -> Result<LedgerState, LedgerManagerError>;
    async fn put_signatures(
        &self,
        signatures: &HashSet<Signature>,
        sn: u64,
        subject_id: &DigestIdentifier,
    ) -> Result<
        (
            u64,
            HashSet<KeyIdentifier>,
            HashSet<KeyIdentifier>,
            LedgerState,
        ),
        LedgerManagerError,
    >;
    async fn init(&self) -> Result<HashMap<DigestIdentifier, LedgerState>, LedgerManagerError>;
    async fn create_event(
        &self,
        request: EventRequest,
        approved: bool,
    ) -> Result<(Event, LedgerState), LedgerManagerError>;
}

pub struct LedgerAPI {
    sender: SenderEnd<CommandManagerMessage, Result<CommandManagerResponse, LedgerManagerError>>,
}

impl LedgerAPI {
    pub fn new(
        sender: SenderEnd<
            CommandManagerMessage,
            Result<CommandManagerResponse, LedgerManagerError>,
        >,
    ) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl LedgerInterface for LedgerAPI {
    async fn get_event(
        &self,
        subject_id: &DigestIdentifier,
        sn: EventSN,
    ) -> Result<(Event, LedgerState), LedgerManagerError> {
        let response = CommandManagerMessage::GetEvent {
            subject_id: subject_id.clone(),
            sn: sn.clone(),
        };
        match self.sender.ask(response).await {
            Ok(data) => match data {
                Ok(response) => {
                    if let CommandManagerResponse::GetEventResponse {
                        event,
                        ledger_state,
                    } = response
                    {
                        Ok((event, ledger_state))
                    } else {
                        panic!("Critical error in LedgerManager implementation")
                    }
                }
                Err(error) => Err(error),
            },
            Err(_) => Err(LedgerManagerError::ChannelClosed),
        }
    }
    async fn get_signatues(
        &self,
        subject_id: DigestIdentifier,
        sn: EventSN,
    ) -> Result<(HashSet<Signature>, LedgerState), LedgerManagerError> {
        let response = CommandManagerMessage::GetSignatures {
            subject_id: subject_id.clone(),
            sn: sn.clone(),
        };
        match self.sender.ask(response).await {
            Ok(data) => match data {
                Ok(response) => {
                    if let CommandManagerResponse::GetSignaturesResponse {
                        signatures,
                        ledger_state,
                    } = response
                    {
                        Ok((signatures, ledger_state))
                    } else {
                        panic!("Critical error in LedgerManager implementation")
                    }
                }
                Err(error) => Err(error),
            },
            Err(_) => Err(LedgerManagerError::ChannelClosed),
        }
    }
    async fn get_signers(
        &self,
        subject_id: DigestIdentifier,
        sn: EventSN,
    ) -> Result<(HashSet<KeyIdentifier>, LedgerState), LedgerManagerError> {
        let response = CommandManagerMessage::GetSigners {
            subject_id: subject_id.clone(),
            sn: sn.clone(),
        };
        match self.sender.ask(response).await {
            Ok(data) => match data {
                Ok(response) => {
                    if let CommandManagerResponse::GetSignersResponse {
                        signers,
                        ledger_state,
                    } = response
                    {
                        Ok((signers, ledger_state))
                    } else {
                        panic!("Critical error in LedgerManager implementation")
                    }
                }
                Err(error) => Err(error),
            },
            Err(_) => Err(LedgerManagerError::ChannelClosed),
        }
    }
    async fn get_subject(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<SubjectData, LedgerManagerError> {
        let response = CommandManagerMessage::GetSubject {
            subject_id: subject_id.clone(),
        };
        match self.sender.ask(response).await {
            Ok(data) => match data {
                Ok(response) => {
                    if let CommandManagerResponse::GetSubjectResponse { subject } = response {
                        Ok(subject)
                    } else {
                        panic!("Critical error in LedgerManager implementation")
                    }
                }
                Err(error) => Err(error),
            },
            Err(_) => Err(LedgerManagerError::ChannelClosed),
        }
    }
    async fn get_subjects(
        &self,
        namespace: String,
    ) -> Result<Vec<SubjectData>, LedgerManagerError> {
        let response = CommandManagerMessage::GetSubjects { namespace };
        match self.sender.ask(response).await {
            Ok(data) => match data {
                Ok(response) => {
                    if let CommandManagerResponse::GetSubjectsResponse { subjects } = response {
                        Ok(subjects)
                    } else {
                        panic!("Critical error in LedgerManager implementation")
                    }
                }
                Err(error) => Err(error),
            },
            Err(_) => Err(LedgerManagerError::ChannelClosed),
        }
    }
    async fn get_subjects_raw(
        &self,
        namespace: String,
    ) -> Result<Vec<Subject>, LedgerManagerError> {
        let response = CommandManagerMessage::GetSubjectsRaw { namespace };
        match self.sender.ask(response).await {
            Ok(data) => match data {
                Ok(response) => {
                    if let CommandManagerResponse::GetSubjectsRawResponse { subjects } = response {
                        Ok(subjects)
                    } else {
                        panic!("Critical error in LedgerManager implementation")
                    }
                }
                Err(error) => Err(error),
            },
            Err(_) => Err(LedgerManagerError::ChannelClosed),
        }
    }
    async fn put_event(&self, event: Event) -> Result<LedgerState, LedgerManagerError> {
        let response = CommandManagerMessage::PutEvent(event);
        match self.sender.ask(response).await {
            Ok(data) => match data {
                Ok(response) => {
                    if let CommandManagerResponse::PutEventResponse { ledger_state } = response {
                        Ok(ledger_state)
                    } else {
                        panic!("Critical error in LedgerManager implementation")
                    }
                }
                Err(error) => Err(error),
            },
            Err(_) => Err(LedgerManagerError::ChannelClosed),
        }
    }
    async fn put_signatures(
        &self,
        signatures: &HashSet<Signature>,
        sn: u64,
        subject_id: &DigestIdentifier,
    ) -> Result<
        (
            u64,
            HashSet<KeyIdentifier>,
            HashSet<KeyIdentifier>,
            LedgerState,
        ),
        LedgerManagerError,
    > {
        let response = CommandManagerMessage::PutSignatures {
            signatures: signatures.clone(),
            sn,
            subject_id: subject_id.clone(),
        };
        match self.sender.ask(response).await {
            Ok(data) => match data {
                Ok(response) => {
                    if let CommandManagerResponse::PutSignaturesResponse {
                        sn,
                        signers,
                        signers_left,
                        ledger_state,
                    } = response
                    {
                        Ok((sn, signers, signers_left, ledger_state))
                    } else {
                        panic!("Critical error in LedgerManager implementation")
                    }
                }
                Err(error) => Err(error),
            },
            Err(_) => Err(LedgerManagerError::ChannelClosed),
        }
    }
    async fn init(&self) -> Result<HashMap<DigestIdentifier, LedgerState>, LedgerManagerError> {
        let response = CommandManagerMessage::Init;
        match self.sender.ask(response).await {
            Ok(data) => match data {
                Ok(response) => {
                    if let CommandManagerResponse::InitResponse(info) = response {
                        Ok(info)
                    } else {
                        panic!("Critical error in LedgerManager implementation")
                    }
                }
                Err(error) => Err(error),
            },
            Err(_) => Err(LedgerManagerError::ChannelClosed),
        }
    }
    async fn create_event(
        &self,
        request: EventRequest,
        approved: bool,
    ) -> Result<(Event, LedgerState), LedgerManagerError> {
        let response = CommandManagerMessage::CreateEvent(request, approved);
        match self.sender.ask(response).await {
            Ok(data) => match data {
                Ok(response) => {
                    if let CommandManagerResponse::CreateEventResponse(event, ledger_state) =
                        response
                    {
                        Ok((event, ledger_state))
                    } else {
                        panic!("Critical error in LedgerManager implementation")
                    }
                }
                Err(error) => Err(error),
            },
            Err(_) => Err(LedgerManagerError::ChannelClosed),
        }
    }
}

pub struct LedgerManager {
    command_input:
        MpscChannel<CommandManagerMessage, Result<CommandManagerResponse, LedgerManagerError>>,
    inner_ledger_manager: Ledger,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl LedgerManager {
    pub fn new(
        command_input: MpscChannel<
            CommandManagerMessage,
            Result<CommandManagerResponse, LedgerManagerError>,
        >,
        gobernance_channel: SenderEnd<GovernanceMessage, GovernanceResponse>,
        repo_access: DB,
        id: KeyIdentifier,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    ) -> Self {
        Self {
            command_input,
            inner_ledger_manager: Ledger::new(
                repo_access,
                id,
                GovernanceAPI::new(gobernance_channel),
            ),
            shutdown_receiver,
        }
    }

    pub async fn start(mut self) {
        loop {
            tokio::select! {
                            msg = self.command_input.receive() => {
                                if let Some(data) = msg {
                                    match data {
                                        ChannelData::TellData(..) => panic!("Received tell in Ledger Manager from Command Manager"),
                ChannelData::AskData(data) => self.process_input(data).await,
            }
                                }
                            },
                            _ = self.shutdown_receiver.recv() => {
                                break;
                            }
                        }
        }
    }

    async fn process_input(
        &mut self,
        data: AskData<CommandManagerMessage, Result<CommandManagerResponse, LedgerManagerError>>,
    ) {
        let (response_channel, data) = data.get();
        match data {
            CommandManagerMessage::Init => response_channel
                .send(self.inner_ledger_manager.init())
                .expect("Channel not closed"),
            CommandManagerMessage::GetEvent { subject_id, sn } => response_channel
                .send(self.inner_ledger_manager.get_event(&subject_id, sn))
                .expect("Channel don't fail"),
            CommandManagerMessage::GetSignatures { subject_id, sn } => response_channel
                .send(self.inner_ledger_manager.get_signatures(&subject_id, sn))
                .expect("Channel don't fail"),
            CommandManagerMessage::GetSigners { subject_id, sn } => response_channel
                .send(self.inner_ledger_manager.get_signers(&subject_id, sn))
                .expect("Channel don't fail"),
            CommandManagerMessage::PutEvent(event) => response_channel
                .send(self.inner_ledger_manager.put_event(event).await)
                .expect("Channel don't fail"),
            CommandManagerMessage::PutSignatures {
                signatures,
                sn,
                subject_id,
            } => response_channel
                .send(
                    self.inner_ledger_manager
                        .put_signatures_top(signatures, sn, subject_id)
                        .await,
                )
                .expect("Channel don't fail"),
            CommandManagerMessage::CreateEvent(event_request, approved) => response_channel
                .send(
                    self.inner_ledger_manager
                        .create_event(event_request, approved)
                        .await,
                )
                .expect("Channel don't fail"),
            CommandManagerMessage::GetSubject { subject_id } => response_channel
                .send(self.inner_ledger_manager.get_subject_top(subject_id))
                .expect("Channel don't fail"),
            CommandManagerMessage::GetSubjects { namespace } => response_channel
                .send(self.inner_ledger_manager.get_subjects(&namespace))
                .expect("Channel don't fail"),
            CommandManagerMessage::GetSubjectsRaw { namespace } => response_channel
                .send(self.inner_ledger_manager.get_subjects_raw(&namespace))
                .expect("Channel don't fail"),
        }
    }
}
// TODO: Check that there are no events in the database with quorum and that no event sourcing has been performed on them (because the execution was stopped in the middle).
// TODO: Delete from the cache of potential candidates those who have a lower sn than the current potential candidate or head

#[cfg(test)]
mod tests {
    use crate::commons::{
        crypto::{Ed25519KeyPair, KeyGenerator, KeyMaterial, KeyPair},
        identifier::{Derivable, KeyIdentifier},
    };
    // use tokio::runtime::Runtime;



    // #[test]
    // fn test_ledger_manager() {
    //     let rt = Runtime::new().unwrap();
    //     rt.block_on(async move {
    //         let temp_dir = TempDir::new("test_simple_insert").unwrap();
    //         let pre_db = open_db(temp_dir.path());
    //         let validators_mc = create_4_validators_mc();
    //         let (sender_command, db, mut gov, ledger_man, mc, ki, _sx) =
    //             create_system(validators_mc.clone(), pre_db.clone());
    //         let mut command_manager = SenderCommand { sender_command };
    //         tokio::spawn(async move {
    //             gov.start().await;
    //         });
    //         tokio::spawn(ledger_man.start());
    //         let init: HashMap<DigestIdentifier, LedgerState> = HashMap::new();
    //         command_manager
    //             .init(Ok(CommandManagerResponse::InitResponse(init)))
    //             .await;
    //         // Create Governance
    //         let event_request_type = EventRequestType::Create(CreateRequest {
    //             governance_id: DigestIdentifier::default(),
    //             schema_id: String::from(""),
    //             namespace: String::from("namespace1"),
    //             payload: RequestPayload::Json(governance_document().to_string()),
    //         });
    //         let event_rt_hash =
    //             DigestIdentifier::from_serializable_borsh(event_request_type.clone()).unwrap();
    //         let signature = Signature {
    //             content: SignatureContent {
    //                 signer: ki.clone(),
    //                 event_content_hash: event_rt_hash.clone(),
    //                 timestamp: Utc::now().timestamp_millis(),
    //             },
    //             signature: SignatureIdentifier {
    //                 derivator: ki.to_signature_derivator(),
    //                 signature: mc
    //                     .sign(Payload::Buffer(event_rt_hash.derivative()))
    //                     .unwrap(),
    //             },
    //         };
    //         let genesis = EventRequest {
    //             request: event_request_type,
    //             signature,
    //             approvals: HashSet::new(),
    //             timestamp: Utc::now().timestamp_millis(),
    //         };
    //         let governance_id = command_manager
    //             .create_event(
    //                 genesis,
    //                 Some(LedgerState {
    //                     head_sn: Some(0),
    //                     head_candidate_sn: None,
    //                     negociating_next: false,
    //                 }),
    //                 None,
    //             )
    //             .await
    //             .unwrap()
    //             .event_content
    //             .subject_id;
    //         let _gov = db.get_subject(&governance_id).expect("Hay subject");
    //         // Create Subject
    //         let event_request_type = EventRequestType::Create(CreateRequest {
    //             governance_id: governance_id.clone(),
    //             schema_id: String::from("prueba"),
    //             namespace: String::from("namespace1"),
    //             payload: RequestPayload::Json(String::from("{\"a\": \"69\"}")),
    //         });
    //         let event_rt_hash =
    //             DigestIdentifier::from_serializable_borsh(event_request_type.clone()).unwrap();
    //         let signature = Signature {
    //             content: SignatureContent {
    //                 signer: ki.clone(),
    //                 event_content_hash: event_rt_hash.clone(),
    //                 timestamp: Utc::now().timestamp_millis(),
    //             },
    //             signature: SignatureIdentifier {
    //                 derivator: ki.to_signature_derivator(),
    //                 signature: mc
    //                     .sign(Payload::Buffer(event_rt_hash.derivative()))
    //                     .unwrap(),
    //             },
    //         };
    //         let genesis = EventRequest {
    //             request: event_request_type,
    //             signature,
    //             approvals: HashSet::new(),
    //             timestamp: Utc::now().timestamp_millis(),
    //         };
    //         let subject_id = command_manager
    //             .create_event(
    //                 genesis,
    //                 Some(LedgerState {
    //                     head_sn: Some(0),
    //                     head_candidate_sn: None,
    //                     negociating_next: false,
    //                 }),
    //                 None,
    //             )
    //             .await
    //             .unwrap()
    //             .event_content
    //             .subject_id;
    //         let subject = db.get_subject(&subject_id).expect("Hay subject");
    //         assert_eq!(
    //             subject.subject_data.unwrap().properties,
    //             String::from("{\"a\": \"69\"}")
    //         );
    //         // Test CreateEvent to create event 1
    //         let event_request_type = EventRequestType::State(StateRequest {
    //             subject_id: subject_id.clone(),
    //             payload: RequestPayload::Json(String::from("{\"a\": \"70\"}")),
    //         });
    //         let event_rt_hash =
    //             DigestIdentifier::from_serializable_borsh(event_request_type.clone()).unwrap();
    //         let signature = Signature {
    //             content: SignatureContent {
    //                 signer: ki.clone(),
    //                 event_content_hash: event_rt_hash.clone(),
    //                 timestamp: Utc::now().timestamp_millis(),
    //             },
    //             signature: SignatureIdentifier {
    //                 derivator: ki.to_signature_derivator(),
    //                 signature: mc
    //                     .sign(Payload::Buffer(event_rt_hash.derivative()))
    //                     .unwrap(),
    //             },
    //         };
    //         let ev1 = EventRequest {
    //             request: event_request_type,
    //             signature,
    //             approvals: HashSet::new(),
    //             timestamp: Utc::now().timestamp_millis(),
    //         };
    //         let _eventoo1 = command_manager
    //             .create_event(
    //                 ev1,
    //                 Some(LedgerState {
    //                     head_sn: Some(0),
    //                     head_candidate_sn: None,
    //                     negociating_next: true,
    //                 }),
    //                 None,
    //             )
    //             .await
    //             .unwrap();
    //         let subject = db.get_subject(&subject_id).expect("Hay subject");
    //         assert_eq!(
    //             subject.subject_data.unwrap().properties,
    //             String::from("{\"a\": \"69\"}")
    //         );
    //         // Create Subject
    //         let event_request_type = EventRequestType::Create(CreateRequest {
    //             governance_id: governance_id.clone(),
    //             schema_id: String::from("prueba"),
    //             namespace: String::from("namespace1"),
    //             payload: RequestPayload::Json(String::from("{\"a\": \"69\"}")),
    //         });
    //         let event_rt_hash =
    //             DigestIdentifier::from_serializable_borsh(event_request_type.clone()).unwrap();
    //         let signature = Signature {
    //             content: SignatureContent {
    //                 signer: ki.clone(),
    //                 event_content_hash: event_rt_hash.clone(),
    //                 timestamp: Utc::now().timestamp_millis(),
    //             },
    //             signature: SignatureIdentifier {
    //                 derivator: ki.to_signature_derivator(),
    //                 signature: mc
    //                     .sign(Payload::Buffer(event_rt_hash.derivative()))
    //                     .unwrap(),
    //             },
    //         };
    //         let genesis = EventRequest {
    //             request: event_request_type,
    //             signature,
    //             approvals: HashSet::new(),
    //             timestamp: Utc::now().timestamp_millis(),
    //         };
    //         let subject_id = command_manager
    //             .create_event(
    //                 genesis,
    //                 Some(LedgerState {
    //                     head_sn: Some(0),
    //                     head_candidate_sn: None,
    //                     negociating_next: false,
    //                 }),
    //                 None,
    //             )
    //             .await
    //             .unwrap()
    //             .event_content
    //             .subject_id;
    //         let subject = db.get_subject(&subject_id).expect("Hay subject");
    //         assert_eq!(
    //             subject.subject_data.unwrap().properties,
    //             String::from("{\"a\": \"69\"}")
    //         );
    //         // Test CreateEvent to create event 1
    //         let event_request_type = EventRequestType::State(StateRequest {
    //             subject_id: subject_id.clone(),
    //             payload: RequestPayload::Json(String::from("{\"a\": \"70\"}")),
    //         });
    //         let event_rt_hash =
    //             DigestIdentifier::from_serializable_borsh(event_request_type.clone()).unwrap();
    //         let signature = Signature {
    //             content: SignatureContent {
    //                 signer: ki.clone(),
    //                 event_content_hash: event_rt_hash.clone(),
    //                 timestamp: Utc::now().timestamp_millis(),
    //             },
    //             signature: SignatureIdentifier {
    //                 derivator: ki.to_signature_derivator(),
    //                 signature: mc
    //                     .sign(Payload::Buffer(event_rt_hash.derivative()))
    //                     .unwrap(),
    //             },
    //         };
    //         let ev1 = EventRequest {
    //             request: event_request_type,
    //             signature,
    //             approvals: HashSet::new(),
    //             timestamp: Utc::now().timestamp_millis(),
    //         };
    //         let eventoo1 = command_manager
    //             .create_event(
    //                 ev1,
    //                 Some(LedgerState {
    //                     head_sn: Some(0),
    //                     head_candidate_sn: None,
    //                     negociating_next: true,
    //                 }),
    //                 None,
    //             )
    //             .await
    //             .unwrap();
    //         let subject = db.get_subject(&subject_id).expect("Hay subject");
    //         assert_eq!(
    //             subject.subject_data.clone().unwrap().properties,
    //             String::from("{\"a\": \"69\"}")
    //         );
    //         assert_eq!(subject_id, eventoo1.event_content.subject_id);
    //         let event1 = db.get_event(&subject_id, 1).expect("Hay evento");
    //         // Create signatures of other validators
    //         let event1_hash =
    //             DigestIdentifier::from_serializable_borsh(event1.event_content.clone()).unwrap();
    //         let signature1 = Signature {
    //             content: SignatureContent {
    //                 signer: validators_mc.1 .1.clone(),
    //                 event_content_hash: event1_hash.clone(),
    //                 timestamp: Utc::now().timestamp_millis(),
    //             },
    //             signature: SignatureIdentifier {
    //                 derivator: validators_mc.1 .1.to_signature_derivator(),
    //                 signature: validators_mc
    //                     .1
    //                      .0
    //                     .sign(Payload::Buffer(event1_hash.derivative()))
    //                     .unwrap(),
    //             },
    //         };
    //         let signature2 = Signature {
    //             content: SignatureContent {
    //                 signer: validators_mc.2 .1.clone(),
    //                 event_content_hash: event1_hash.clone(),
    //                 timestamp: Utc::now().timestamp_millis(),
    //             },
    //             signature: SignatureIdentifier {
    //                 derivator: validators_mc.2 .1.to_signature_derivator(),
    //                 signature: validators_mc
    //                     .2
    //                      .0
    //                     .sign(Payload::Buffer(event1_hash.derivative()))
    //                     .unwrap(),
    //             },
    //         };
    //         let mut signatures1 = HashSet::new();
    //         signatures1.insert(signature1);
    //         let mut signers = HashSet::new();
    //         let mut signers_left = HashSet::new();
    //         signers.insert(validators_mc.1 .1.clone());
    //         signers_left.insert(validators_mc.3 .1.clone());
    //         signers_left.insert(validators_mc.0 .1.clone());
    //         signers_left.insert(validators_mc.2 .1.clone());
    //         command_manager
    //             .put_signatures(
    //                 signatures1.clone(),
    //                 1,
    //                 subject_id.clone(),
    //                 Ok(CommandManagerResponse::PutSignaturesResponse {
    //                     ledger_state: LedgerState {
    //                         head_sn: Some(0),
    //                         head_candidate_sn: None,
    //                         negociating_next: true,
    //                     },
    //                     sn: 1,
    //                     signers: signers.clone(),
    //                     signers_left: signers_left.clone(),
    //                 }),
    //             )
    //             .await;
    //         signatures1.insert(signature2);
    //         let mut signers = HashSet::new();
    //         let mut signers_left = HashSet::new();
    //         signers.insert(validators_mc.1 .1.clone());
    //         signers.insert(validators_mc.2 .1.clone());
    //         signers_left.insert(validators_mc.3 .1.clone());
    //         signers_left.insert(validators_mc.0 .1.clone());
    //         command_manager
    //             .put_signatures(
    //                 signatures1.clone(),
    //                 1,
    //                 subject_id.clone(),
    //                 Ok(CommandManagerResponse::PutSignaturesResponse {
    //                     ledger_state: LedgerState {
    //                         head_sn: Some(1),
    //                         head_candidate_sn: None,
    //                         negociating_next: false,
    //                     },
    //                     sn: 1,
    //                     signers: signers.clone(),
    //                     signers_left: signers_left.clone(),
    //                 }),
    //             )
    //             .await;
    //         let subject = db.get_subject(&subject_id).expect("Hay subject");
    //         assert_eq!(
    //             subject.subject_data.clone().unwrap().properties,
    //             String::from("{\"a\": \"70\"}")
    //         );
    //         // Event 2
    //         let event_request_type = EventRequestType::State(StateRequest {
    //             subject_id: subject_id.clone(),
    //             payload: RequestPayload::JsonPatch(String::from(
    //                 r#"[{ "op": "replace", "path": "/a", "value": "71" }]"#,
    //             )),
    //         });
    //         let event_rt_hash =
    //             DigestIdentifier::from_serializable_borsh(event_request_type.clone()).unwrap();
    //         let signature = Signature {
    //             content: SignatureContent {
    //                 signer: ki.clone(),
    //                 event_content_hash: event_rt_hash.clone(),
    //                 timestamp: Utc::now().timestamp_millis(),
    //             },
    //             signature: SignatureIdentifier {
    //                 derivator: ki.to_signature_derivator(),
    //                 signature: mc
    //                     .sign(Payload::Buffer(event_rt_hash.derivative()))
    //                     .unwrap(),
    //             },
    //         };
    //         let ev1 = EventRequest {
    //             request: event_request_type,
    //             signature,
    //             approvals: HashSet::new(),
    //             timestamp: Utc::now().timestamp_millis(),
    //         };
    //         let eventoo1 = command_manager
    //             .create_event(
    //                 ev1,
    //                 Some(LedgerState {
    //                     head_sn: Some(1),
    //                     head_candidate_sn: None,
    //                     negociating_next: true,
    //                 }),
    //                 None,
    //             )
    //             .await
    //             .unwrap();
    //         let event1 = db.get_event(&subject_id, 2).expect("Hay evento");
    //         assert_eq!(eventoo1, event1);
    //         // Create signatures of other validators
    //         let event1_hash =
    //             DigestIdentifier::from_serializable_borsh(event1.event_content.clone()).unwrap();
    //         assert_eq!(eventoo1.signature.content.event_content_hash, event1_hash);
    //         assert_eq!(event1.signature.content.event_content_hash, event1_hash);
    //         let signature1 = Signature {
    //             content: SignatureContent {
    //                 signer: validators_mc.1 .1.clone(),
    //                 event_content_hash: event1_hash.clone(),
    //                 timestamp: Utc::now().timestamp_millis(),
    //             },
    //             signature: SignatureIdentifier {
    //                 derivator: validators_mc.1 .1.to_signature_derivator(),
    //                 signature: validators_mc
    //                     .1
    //                      .0
    //                     .sign(Payload::Buffer(event1_hash.derivative()))
    //                     .unwrap(),
    //             },
    //         };
    //         let signature2 = Signature {
    //             content: SignatureContent {
    //                 signer: validators_mc.2 .1.clone(),
    //                 event_content_hash: event1_hash.clone(),
    //                 timestamp: Utc::now().timestamp_millis(),
    //             },
    //             signature: SignatureIdentifier {
    //                 derivator: validators_mc.2 .1.to_signature_derivator(),
    //                 signature: validators_mc
    //                     .2
    //                      .0
    //                     .sign(Payload::Buffer(event1_hash.derivative()))
    //                     .unwrap(),
    //             },
    //         };
    //         let mut signatures2 = HashSet::new();
    //         signatures2.insert(signature1);
    //         signatures2.insert(signature2);
    //         command_manager
    //             .put_signatures(
    //                 signatures2.clone(),
    //                 2,
    //                 subject_id.clone(),
    //                 Ok(CommandManagerResponse::PutSignaturesResponse {
    //                     ledger_state: LedgerState {
    //                         head_sn: Some(2),
    //                         head_candidate_sn: None,
    //                         negociating_next: false,
    //                     },
    //                     sn: 2,
    //                     signers: signers.clone(),
    //                     signers_left: signers_left.clone(),
    //                 }),
    //             )
    //             .await;
    //         let subject = db.get_subject(&subject_id).expect("Hay subject");
    //         assert_eq!(
    //             subject.subject_data.clone().unwrap().properties,
    //             String::from("{\"a\":\"71\"}")
    //         );
    //         // Event 3
    //         let event_request_type = EventRequestType::State(StateRequest {
    //             subject_id: subject_id.clone(),
    //             payload: RequestPayload::Json(String::from("{\"a\": \"72\"}")),
    //         });
    //         let event_rt_hash =
    //             DigestIdentifier::from_serializable_borsh(event_request_type.clone()).unwrap();
    //         let signature = Signature {
    //             content: SignatureContent {
    //                 signer: ki.clone(),
    //                 event_content_hash: event_rt_hash.clone(),
    //                 timestamp: Utc::now().timestamp_millis(),
    //             },
    //             signature: SignatureIdentifier {
    //                 derivator: ki.to_signature_derivator(),
    //                 signature: mc
    //                     .sign(Payload::Buffer(event_rt_hash.derivative()))
    //                     .unwrap(),
    //             },
    //         };
    //         let ev1 = EventRequest {
    //             request: event_request_type,
    //             signature,
    //             approvals: HashSet::new(),
    //             timestamp: Utc::now().timestamp_millis(),
    //         };
    //         let eventoo1 = command_manager
    //             .create_event(
    //                 ev1,
    //                 Some(LedgerState {
    //                     head_sn: Some(2),
    //                     head_candidate_sn: None,
    //                     negociating_next: true,
    //                 }),
    //                 None,
    //             )
    //             .await
    //             .unwrap();
    //         let event1 = db.get_event(&subject_id, 3).expect("Hay evento");
    //         assert_eq!(eventoo1, event1);
    //         // Create signatures of other validators
    //         let event1_hash =
    //             DigestIdentifier::from_serializable_borsh(event1.event_content.clone()).unwrap();
    //         assert_eq!(eventoo1.signature.content.event_content_hash, event1_hash);
    //         assert_eq!(event1.signature.content.event_content_hash, event1_hash);
    //         let signature1 = Signature {
    //             content: SignatureContent {
    //                 signer: validators_mc.1 .1.clone(),
    //                 event_content_hash: event1_hash.clone(),
    //                 timestamp: Utc::now().timestamp_millis(),
    //             },
    //             signature: SignatureIdentifier {
    //                 derivator: validators_mc.1 .1.to_signature_derivator(),
    //                 signature: validators_mc
    //                     .1
    //                      .0
    //                     .sign(Payload::Buffer(event1_hash.derivative()))
    //                     .unwrap(),
    //             },
    //         };
    //         let signature2 = Signature {
    //             content: SignatureContent {
    //                 signer: validators_mc.2 .1.clone(),
    //                 event_content_hash: event1_hash.clone(),
    //                 timestamp: Utc::now().timestamp_millis(),
    //             },
    //             signature: SignatureIdentifier {
    //                 derivator: validators_mc.2 .1.to_signature_derivator(),
    //                 signature: validators_mc
    //                     .2
    //                      .0
    //                     .sign(Payload::Buffer(event1_hash.derivative()))
    //                     .unwrap(),
    //             },
    //         };
    //         let mut signatures3 = HashSet::new();
    //         signatures3.insert(signature1);
    //         signatures3.insert(signature2);
    //         signers_left.insert(validators_mc.0 .1.clone());
    //         command_manager
    //             .put_signatures(
    //                 signatures3.clone(),
    //                 3,
    //                 subject_id.clone(),
    //                 Ok(CommandManagerResponse::PutSignaturesResponse {
    //                     ledger_state: LedgerState {
    //                         head_sn: Some(3),
    //                         head_candidate_sn: None,
    //                         negociating_next: false,
    //                     },
    //                     sn: 3,
    //                     signers: signers.clone(),
    //                     signers_left: signers_left.clone(),
    //                 }),
    //             )
    //             .await;
    //         let subject = db.get_subject(&subject_id).expect("Hay subject");
    //         assert_eq!(
    //             subject.subject_data.clone().unwrap().properties,
    //             String::from("{\"a\": \"72\"}")
    //         );
    //         let (sender_command, _db2, mut gov, ledger_man, _mc, _ki, _sx) =
    //             create_system(validators_mc.clone(), pre_db);
    //         let mut command_manager = SenderCommand { sender_command };
    //         tokio::spawn(async move {
    //             gov.start().await;
    //         });
    //         tokio::spawn(ledger_man.start());
    //         command_manager
    //             .get_event(
    //                 subject_id.clone(),
    //                 EventSN::HEAD,
    //                 Err(LedgerManagerError::SubjectNotFound),
    //             )
    //             .await;
    //         command_manager
    //             .get_signatures(
    //                 subject_id.clone(),
    //                 EventSN::HEAD,
    //                 Err(LedgerManagerError::SubjectNotFound),
    //             )
    //             .await;
    //         command_manager
    //             .get_signers(
    //                 subject_id.clone(),
    //                 EventSN::HEAD,
    //                 Err(LedgerManagerError::SubjectNotFound),
    //             )
    //             .await;
    //         let event = db.get_event(&subject_id, 0).unwrap();
    //         command_manager
    //             .put_event(
    //                 event,
    //                 Ok(CommandManagerResponse::PutEventResponse {
    //                     ledger_state: LedgerState {
    //                         head_sn: Some(0),
    //                         head_candidate_sn: None,
    //                         negociating_next: false,
    //                     },
    //                 }),
    //             )
    //             .await;
    //         let event = db.get_event(&subject_id, 1).unwrap();
    //         command_manager
    //             .put_event(
    //                 event,
    //                 Ok(CommandManagerResponse::PutEventResponse {
    //                     ledger_state: LedgerState {
    //                         head_sn: Some(0),
    //                         head_candidate_sn: None,
    //                         negociating_next: true,
    //                     },
    //                 }),
    //             )
    //             .await;
    //         command_manager
    //             .put_signatures(
    //                 signatures1.clone(),
    //                 1,
    //                 subject_id.clone(),
    //                 Ok(CommandManagerResponse::PutSignaturesResponse {
    //                     ledger_state: LedgerState {
    //                         head_sn: Some(1),
    //                         head_candidate_sn: None,
    //                         negociating_next: false,
    //                     },
    //                     sn: 1,
    //                     signers: signers.clone(),
    //                     signers_left: signers_left.clone(),
    //                 }),
    //             )
    //             .await;
    //         let event = db.get_event(&subject_id, 3).unwrap();
    //         command_manager
    //             .put_event(
    //                 event,
    //                 Ok(CommandManagerResponse::PutEventResponse {
    //                     ledger_state: LedgerState {
    //                         head_sn: Some(1),
    //                         head_candidate_sn: None,
    //                         negociating_next: false,
    //                     },
    //                 }),
    //             )
    //             .await;
    //         command_manager
    //             .put_signatures(
    //                 signatures3.clone(),
    //                 3,
    //                 subject_id.clone(),
    //                 Ok(CommandManagerResponse::PutSignaturesResponse {
    //                     ledger_state: LedgerState {
    //                         head_sn: Some(1),
    //                         head_candidate_sn: Some(3),
    //                         negociating_next: false,
    //                     },
    //                     sn: 3,
    //                     signers: signers.clone(),
    //                     signers_left: signers_left.clone(),
    //                 }),
    //             )
    //             .await;
    //         let event = db.get_event(&subject_id, 2).unwrap();
    //         command_manager
    //             .put_event(
    //                 event,
    //                 Ok(CommandManagerResponse::PutEventResponse {
    //                     ledger_state: LedgerState {
    //                         head_sn: Some(3),
    //                         head_candidate_sn: None,
    //                         negociating_next: false,
    //                     },
    //                 }),
    //             )
    //             .await;
    //     });
    // }

    #[test]
    fn test_lll() {
        let mcs = create_4_validators_mc();
        println!("MC0: {:?}", mcs.0 .1.to_str());
        println!("MC1: {:?}", mcs.1 .1.to_str());
        println!("MC2: {:?}", mcs.2 .1.to_str());
        println!("MC3: {:?}", mcs.3 .1.to_str());
    }

    // fn governance_document() -> Value {
    //     json!({
    //             "members": [
    //                 {
    //                     "id": "Open Canarias",
    //                     "tags": {},
    //                     "description": "a",
    //                     "key": "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w",
    //                 },
    //                 {
    //                     "id": "Acciona",
    //                     "tags": {},
    //                     "description": "b",
    //                     "key": "ECQnl-h1vEWmu-ZlPuweR3N1x6SUImyVdPrCLmnJJMyU",
    //                 },
    //                 {
    //                     "id": "Iberdrola",
    //                     "tags": {},
    //                     "description": "c",
    //                     "key": "EejcG-XG-dR991FEGR2Y3PefeKa5v0yTOXl80azRwgOk",
    //                 },
    //                 {
    //                     "id": "Ford",
    //                     "tags": {},
    //                     "description": "d",
    //                     "key": "EV0iN392n8rj7WoBfUWr5B9AAUt04ocQ2r-g271UyPqw",
    //                 },
    //             ],
    //             "schemas": [
    //                 {
    //                     "id": "prueba",
    //                     "tags": {},
    //                     "content": {
    //                         "a": {"type": "string"}
    //                     },
    //                 }
    //             ],
    //     })
    // }

    // TODO: Test with something in the database to check Init

    fn create_4_validators_mc() -> (
        (KeyPair, KeyIdentifier),
        (KeyPair, KeyIdentifier),
        (KeyPair, KeyIdentifier),
        (KeyPair, KeyIdentifier),
    ) {
        let mc1 = KeyPair::Ed25519(Ed25519KeyPair::from_seed(String::from("40000").as_bytes()));
        let mc2 = KeyPair::Ed25519(Ed25519KeyPair::from_seed(String::from("40001").as_bytes()));
        let mc3 = KeyPair::Ed25519(Ed25519KeyPair::from_seed(String::from("40002").as_bytes()));
        let mc4 = KeyPair::Ed25519(Ed25519KeyPair::from_seed(String::from("40003").as_bytes()));
        (
            (
                mc1.clone(),
                KeyIdentifier::new(mc1.get_key_derivator(), &mc1.public_key_bytes()),
            ),
            (
                mc2.clone(),
                KeyIdentifier::new(mc2.get_key_derivator(), &mc2.public_key_bytes()),
            ),
            (
                mc3.clone(),
                KeyIdentifier::new(mc3.get_key_derivator(), &mc3.public_key_bytes()),
            ),
            (
                mc4.clone(),
                KeyIdentifier::new(mc4.get_key_derivator(), &mc4.public_key_bytes()),
            ),
        )
    }

    // // use tempdir::TempDir;

    // fn create_system(
    //     keys: (
    //         (KeyPair, KeyIdentifier),
    //         (KeyPair, KeyIdentifier),
    //         (KeyPair, KeyIdentifier),
    //         (KeyPair, KeyIdentifier),
    //     ),
    //     pre_db: Arc<Database<StringKey>>,
    // ) -> (
    //     SenderEnd<CommandManagerMessage, Result<CommandManagerResponse, LedgerManagerError>>,
    //     DB,
    //     Governance,
    //     LedgerManager,
    //     KeyPair,
    //     KeyIdentifier,
    //     tokio::sync::broadcast::Sender<()>,
    // ) {
    //     let _validators_list = vec![
    //         GovernanceMember {
    //             id: String::from("Open Canarias"),
    //             namespace: String::from("namespace1"),
    //             description: String::from("description"),
    //             key: keys.0 .1.clone(),
    //         },
    //         GovernanceMember {
    //             id: String::from("Acciona"),
    //             namespace: String::from("namespace1"),
    //             description: String::from("description"),
    //             key: keys.1 .1.clone(),
    //         },
    //         GovernanceMember {
    //             id: String::from("Iberdrola"),
    //             namespace: String::from("namespace1"),
    //             description: String::from("description"),
    //             key: keys.2 .1.clone(),
    //         },
    //         GovernanceMember {
    //             id: String::from("Ford"),
    //             namespace: String::from("namespace1"),
    //             description: String::from("description"),
    //             key: keys.3 .1.clone(),
    //         },
    //     ];
    //     let (input_command, sender_command) = MpscChannel::<
    //         CommandManagerMessage,
    //         Result<CommandManagerResponse, LedgerManagerError>,
    //     >::new(100);
    //     let (input_gov, sender_gov) =
    //         MpscChannel::<GovernanceMessage, GovernanceResponse>::new(100);
    //     let (brx, bsx) = tokio::sync::broadcast::channel::<()>(10);
    //     let gov = Governance::new(input_gov, brx, bsx, DB::new(pre_db.clone()));
    //     let mc = KeyPair::Ed25519(Ed25519KeyPair::from_seed(String::from("40000").as_bytes()));
    //     let id = KeyIdentifier::new(mc.get_key_derivator(), &mc.public_key_bytes());
    //     let (brs, brx) = tokio::sync::broadcast::channel::<()>(10);
    //     let ledger_manager = LedgerManager::new(
    //         input_command,
    //         sender_gov,
    //         DB::new(pre_db.clone()),
    //         id.clone(),
    //         brx,
    //     );
    //     (
    //         sender_command,
    //         DB::new(pre_db),
    //         gov,
    //         ledger_manager,
    //         mc,
    //         id,
    //         brs,
    //     )
    // }

    // struct SenderCommand {
    //     sender_command:
    //         SenderEnd<CommandManagerMessage, Result<CommandManagerResponse, LedgerManagerError>>,
    // }

    // impl SenderCommand {
    //     pub async fn get_event(
    //         &mut self,
    //         subject_id: DigestIdentifier,
    //         sn: EventSN,
    //         cmr: Result<CommandManagerResponse, LedgerManagerError>,
    //     ) {
    //         let cm = CommandManagerMessage::GetEvent { subject_id, sn };
    //         match self.sender_command.ask(cm).await {
    //             Ok(resp) => {
    //                 assert_eq!(resp, cmr);
    //             }
    //             Err(_) => panic!("a"),
    //         };
    //     }

    //     pub async fn get_signatures(
    //         &mut self,
    //         subject_id: DigestIdentifier,
    //         sn: EventSN,
    //         cmr: Result<CommandManagerResponse, LedgerManagerError>,
    //     ) {
    //         let cm = CommandManagerMessage::GetSignatures { subject_id, sn };
    //         match self.sender_command.ask(cm).await {
    //             Ok(resp) => {
    //                 assert_eq!(resp, cmr);
    //             }
    //             Err(_) => panic!("a"),
    //         };
    //     }

    //     pub async fn get_signers(
    //         &mut self,
    //         subject_id: DigestIdentifier,
    //         sn: EventSN,
    //         cmr: Result<CommandManagerResponse, LedgerManagerError>,
    //     ) {
    //         let cm = CommandManagerMessage::GetSigners { subject_id, sn };
    //         match self.sender_command.ask(cm).await {
    //             Ok(resp) => {
    //                 assert_eq!(resp, cmr);
    //             }
    //             Err(_) => panic!("a"),
    //         };
    //     }

    //     pub async fn put_event(
    //         &mut self,
    //         event: Event,
    //         cmr: Result<CommandManagerResponse, LedgerManagerError>,
    //     ) {
    //         let cm = CommandManagerMessage::PutEvent(event);
    //         match self.sender_command.ask(cm).await {
    //             Ok(resp) => {
    //                 assert_eq!(resp, cmr);
    //             }
    //             Err(_) => panic!("a"),
    //         };
    //     }

    //     pub async fn put_signatures(
    //         &mut self,
    //         signatures: HashSet<Signature>,
    //         sn: u64,
    //         subject_id: DigestIdentifier,
    //         cmr: Result<CommandManagerResponse, LedgerManagerError>,
    //     ) {
    //         let cm = CommandManagerMessage::PutSignatures {
    //             signatures,
    //             subject_id,
    //             sn,
    //         };
    //         match self.sender_command.ask(cm).await {
    //             Ok(resp) => {
    //                 assert_eq!(resp, cmr);
    //             }
    //             Err(_) => panic!("a"),
    //         };
    //     }

    //     pub async fn init(&mut self, cmr: Result<CommandManagerResponse, LedgerManagerError>) {
    //         let cm = CommandManagerMessage::Init;
    //         match self.sender_command.ask(cm).await {
    //             Ok(resp) => {
    //                 assert_eq!(resp, cmr);
    //             }
    //             Err(_) => panic!("a"),
    //         };
    //     }
    //     pub async fn create_event(
    //         &mut self,
    //         event_request: EventRequest,
    //         ledger_state: Option<LedgerState>,
    //         error: Option<SubjectError>,
    //     ) -> Option<Event> {
    //         let cm = CommandManagerMessage::CreateEvent(event_request, true);
    //         match self.sender_command.ask(cm).await {
    //             Ok(resp) => match resp {
    //                 Ok(CommandManagerResponse::CreateEventResponse(event, le)) => {
    //                     assert_eq!(le, ledger_state.unwrap());
    //                     Some(event)
    //                 }
    //                 Err(e) => {
    //                     if let LedgerManagerError::SubjectError(se) = e {
    //                         println!("{:?}", se);
    //                         assert_eq!(se, error.unwrap());
    //                     } else {
    //                         panic!("aaaasasdfasfasasdfasaaaa");
    //                     }
    //                     None
    //                 }
    //                 _ => panic!("how"),
    //             },
    //             Err(_) => panic!("a"),
    //         }
    //     }
    // }
}
