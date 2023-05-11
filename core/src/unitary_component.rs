use std::sync::Arc;

use crate::approval::manager::{ApprovalAPI, ApprovalManager};
use crate::approval::{ApprovalMessages, ApprovalResponses};
use crate::commons::channel::MpscChannel;
use crate::commons::config::NetworkSettings;
use crate::commons::config::{DatabaseSettings, NodeSettings, TapleSettings};
use crate::commons::crypto::{
    Ed25519KeyPair, KeyGenerator, KeyMaterial, KeyPair, Secp256k1KeyPair,
};
use crate::commons::identifier::derive::KeyDerivator;
use crate::commons::identifier::{Derivable, KeyIdentifier};
use crate::commons::models::notification::Notification;
use crate::commons::self_signature_manager::{SelfSignatureInterface, SelfSignatureManager};
use crate::database::{DatabaseManager, DB};
use crate::distribution::error::DistributionErrorResponses;
use crate::distribution::manager::DistributionManager;
use crate::distribution::DistributionMessagesNew;
use crate::evaluator::{EvaluatorManager, EvaluatorMessage, EvaluatorResponse};
use crate::event::manager::{EventAPI, EventManager};
use crate::event::{EventCommand, EventResponse};
use crate::governance::GovernanceAPI;
use crate::governance::{governance::Governance, GovernanceMessage, GovernanceResponse};
use crate::ledger::{manager::EventManager as LedgerManager, LedgerCommand, LedgerResponse};
use crate::message::{
    Message, MessageReceiver, MessageSender, MessageTaskCommand, MessageTaskManager, NetworkEvent,
};
use crate::network::network::{NetworkProcessor, SendMode};
use crate::notary::manager::NotaryManager;
use crate::notary::{NotaryCommand, NotaryResponse};
use crate::protocol::protocol_message_manager::{ProtocolManager, TapleMessages};
use futures::future::BoxFuture;
use futures::FutureExt;
use libp2p::{Multiaddr, PeerId};
use tokio::sync::broadcast::error::{RecvError, TryRecvError};

use crate::api::{APICommands, ApiResponses, NodeAPI, API};
use crate::error::Error;

const BUFFER_SIZE: usize = 1000;

/// Object that allows receiving [notifications](Notification) of the
/// different events of relevance that a node performs and/or detects.
///
/// These objects can only be obtained through a node that has already been initialized.
/// In case of multiple nodes, the same handler cannot be used to obtain
/// notifications from each of them. Instead, one must be instantiated for each node in the
/// application and they will only be able to receive notifications from that point on,
/// the previous ones being unrecoverable.
pub struct NotificationHandler {
    notification_receiver: tokio::sync::broadcast::Receiver<Notification>,
}

impl NotificationHandler {
    /// It forces the object to wait until the arrival of a new notification.
    /// It is important to note that handlers have an internal queue for storing messages.
    /// This queue starts acting from the moment the object is created, allowing the object
    /// to retrieve notifications from that moment until the current one. In this case,
    /// the method returns instantly with the oldest notification.
    ///
    /// An `Error` will only be obtained if it is not possible to receive more notifications
    /// due to a node stop and if there are no messages queued. In such a situation,
    /// the handler becomes useless and its release from memory is recommended.
    pub fn receive<'a>(&'a mut self) -> BoxFuture<'a, Result<Notification, Error>> {
        async move {
            loop {
                match self.notification_receiver.recv().await {
                    Ok(value) => break Ok(value),
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break Err(Error::CantReceiveNotification),
                }
            }
        }
        .boxed()
    }

    /// The handler tries to get a notification. If there is none, it returns instead of waiting.
    /// Because of this, this method can be used to determine if the notification queue is empty,
    /// since it will report such a possibility with an error.
    ///
    /// # Possible results
    /// • A notification will be obtained only if it exists in the object's queue. <br />
    /// • [Error::CantReceiveNotification] will be obtained if it is not possible to receive more notifications. <br />
    /// • [Error::NoNewNotification] will be obtained if there is no message queued and it is still possible to
    /// continue receiving messages.
    pub fn try_rec(&mut self) -> Result<Notification, Error> {
        loop {
            match self.notification_receiver.try_recv() {
                Ok(value) => break Ok(value),
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Closed) => break Err(Error::CantReceiveNotification),
                Err(TryRecvError::Empty) => break Err(Error::NoNewNotification),
            }
        }
    }
}

/// Structure representing a node of a TAPLE network.
///
/// A node must be instantiated using the [`Taple::new`] method, which requires a set
/// of [configuration](Settings) parameters in order to be properly initialized.
///
#[derive(Debug)]
pub struct Taple<D: DatabaseManager> {
    api: NodeAPI,
    peer_id: Option<PeerId>,
    controller_id: Option<String>,
    public_key: Option<Vec<u8>>,
    api_input: Option<MpscChannel<APICommands, ApiResponses>>,
    notification_sender: tokio::sync::broadcast::Sender<Notification>,
    settings: TapleSettings,
    database: Option<D>,
}

impl<D: DatabaseManager + 'static> Taple<D> {
    /// Returns the [PeerId] of the node is available.
    /// This ID is the identifier of the node at the network level.
    /// **None** can only be get if the node has not been started yet.
    pub fn peer_id(&self) -> Option<PeerId> {
        self.peer_id.clone()
    }

    /// Returns the public key (bytes format) of the node is available.
    /// **None** can only be get if the node has not been started yet.
    pub fn public_key(&self) -> Option<Vec<u8>> {
        self.public_key.clone()
    }

    /// Returns the controller ID of the node is available.
    /// This ID is the identifier of the node at the protocol level.
    /// **None** can only be get if the node has not been started yet.
    pub fn controller_id(&self) -> Option<String> {
        self.controller_id.clone()
    }

    /// This methods allows to get the [API](NodeAPI) of the node. The API can be get
    /// as many time as desired. The API is the only method to interact with a node at the user level.
    pub fn get_api(&self) -> NodeAPI {
        self.api.clone()
    }

    /// This method allows to get an instance of [NotificationHandler].
    /// This component is used by the node to report any important events
    /// that have occurred, for example the creation of new **subjects**.
    /// The component behaves similar to a channel receiver; users only have to call
    /// the [NotificationHandler::receive] method to start receiving notifications.
    pub fn get_notification_handler(&self) -> NotificationHandler {
        NotificationHandler {
            notification_receiver: self.notification_sender.subscribe(),
        }
    }

    /// This method allows the creation of cryptographic material through a
    /// given public key.
    fn generate_mc(&mut self, stored_public_key: Option<String>) -> Result<KeyPair, Error> {
        let kp = Self::create_key_pair(
            &self.settings.node.key_derivator,
            self.settings.node.seed.clone(),
            self.settings.node.secret_key.clone(),
        )?;
        let public_key = kp.public_key_bytes();
        let key_identifier = KeyIdentifier::new(kp.get_key_derivator(), &public_key).to_str();
        if let Some(key) = stored_public_key {
            if (key_identifier != key) && !self.settings.node.dev_mode {
                log::error!("Invalid MC specified. There is a previous defined MC in the system");
                return Err(Error::InvalidKeyPairSpecified(key_identifier));
            }
        }
        self.controller_id = Some(key_identifier);
        self.public_key = Some(public_key);
        Ok(kp)
    }

    /// Main and unique method to create an instance of a TAPLE node.
    pub fn new(settings: TapleSettings, database: D) -> Self {
        check_dev_settings(&settings);
        let (api_input, api_sender) = MpscChannel::new(BUFFER_SIZE);
        let (sender, _) = tokio::sync::broadcast::channel(BUFFER_SIZE);
        let api = NodeAPI { sender: api_sender };
        Self {
            api,
            peer_id: None,
            public_key: None,
            controller_id: None,
            api_input: Some(api_input),
            notification_sender: sender,
            settings,
            database: Some(database),
        }
    }

    /// Instance a default settings to start a new Taple Node
    pub fn get_default_settings() -> TapleSettings {
        TapleSettings {
            network: NetworkSettings {
                p2p_port: 50000u32,
                addr: "/ip4/0.0.0.0/tcp".into(),
                known_nodes: Vec::<String>::new(),
            },
            node: NodeSettings {
                key_derivator: KeyDerivator::Ed25519,
                secret_key: Option::<String>::None,
                seed: None,
                digest_derivator:
                    crate::commons::identifier::derive::digest::DigestDerivator::Blake3_256,
                replication_factor: 0.25f64,
                timeout: 3000u32,
                passvotation: 0,
                dev_mode: false,
                smartcontracts_directory: "../../../contracts".into(),
                req_res: false,
            },
            database: DatabaseSettings { path: "".into() },
        }
    }

    // // Instance a default governance settings
    // pub fn get_default_governance(&self) -> RequestPayload {
    //     RequestPayload::Json(
    //         serde_json::to_string(&serde_json::json!({
    //                             "members": [
    //                                 {
    //                                     "id": "Company",
    //                                     "tags": {},
    //                                     "description": "Basic Usage",
    //                                     "key": self.controller_id().unwrap()
    //                                 },
    //                             ],
    //                             "schemas": [],
    //                             "policies": [
    //                                 {
    //                                     "id": "governance",
    //                                     "validation": {
    //                                         "quorum": 0.5,
    //                                         "validators": [
    //                                             self.controller_id().unwrap()
    //                                         ]
    //                                     },
    //                                     "approval": {
    //                                         "quorum": 0.5,
    //                                         "approvers": [
    //         ]
    //                                     },
    //                                     "invokation": {
    //                                         "owner": {
    //                                             "allowance": true,
    //                                             "approvalRequired": false
    //                                         },
    //                                         "set": {
    //                                             "allowance": false,
    //                                             "approvalRequired": false,
    //                                             "invokers": []
    //                                         },
    //                                         "all": {
    //                                             "allowance": false,
    //                                             "approvalRequired": false,
    //                                         },
    //                                         "external": {
    //                                             "allowance": false,
    //                                             "approvalRequired": false
    //                                         }
    //                                     }
    //                                 }
    //                             ]
    //                     }))
    //         .unwrap(),
    //     )
    // }

    /// This method initializes a TAPLE node, generating each of its internal components
    /// and allowing subsequent interaction with the node. Each of the aforementioned
    /// components is executed in its own Tokyo task, allowing the method to return the
    /// control flow once its execution is finished.
    /// # Possible results
    /// If the process is successful, the method will return `Ok(())`.
    /// An error will be returned only if it has not been possible to generate the necessary data
    /// for the initialization of the components, mainly due to problems in the initial [configuration](Settings).
    /// # Panics
    /// This method panics if it has not been possible to generate the network layer.
    pub async fn start(&mut self) -> Result<(), Error> {
        // Create channels
        // Channels for network
        let (sender_network, receiver_network): (
            tokio::sync::mpsc::Sender<NetworkEvent>,
            tokio::sync::mpsc::Receiver<NetworkEvent>,
        ) = tokio::sync::mpsc::channel(BUFFER_SIZE);
        let (event_receiver, event_sender) =
            MpscChannel::<EventCommand, EventResponse>::new(BUFFER_SIZE);
        // Receiver and sender of commands
        let (ledger_receiver, ledger_sender) =
            MpscChannel::<LedgerCommand, LedgerResponse>::new(BUFFER_SIZE);
        // Receiver and sender of governance messages
        let (governance_receiver, governance_sender) =
            MpscChannel::<GovernanceMessage, GovernanceResponse>::new(BUFFER_SIZE);
        // Governance notification channel
        let (governance_update_sx, governance_update_rx) =
            tokio::sync::broadcast::channel(BUFFER_SIZE);
        // Receiver and sender of ledger message
        // Receiver and sender of taskManager requests
        let (task_receiver, task_sender) =
            MpscChannel::<MessageTaskCommand<TapleMessages>, ()>::new(BUFFER_SIZE);
        // Receiver and sender of protocol messages
        let (protocol_receiver, protocol_sender) =
            MpscChannel::<Message<TapleMessages>, ()>::new(BUFFER_SIZE);
        // Receiver and sender of distribution messages
        let (distribution_receiver, distribution_sender) = MpscChannel::<
            DistributionMessagesNew,
            Result<(), DistributionErrorResponses>,
        >::new(BUFFER_SIZE);
        // Receiver and sender of approval messages
        let (approval_receiver, approval_sender) =
            MpscChannel::<ApprovalMessages, ApprovalResponses>::new(BUFFER_SIZE);
        // Receiver and sender of evaluation messages
        let (evaluation_receiver, evaluation_sender) =
            MpscChannel::<EvaluatorMessage, EvaluatorResponse>::new(BUFFER_SIZE);
        // Receiver and sender of validation messages
        let (validation_receiver, validation_sender) =
            MpscChannel::<NotaryCommand, NotaryResponse>::new(BUFFER_SIZE);
        // Shutdown channel
        let (bsx, _brx) = tokio::sync::broadcast::channel::<()>(10);
        // Creation Watch Channel
        let (wath_sender, watch_receiver): (
            tokio::sync::watch::Sender<TapleSettings>,
            tokio::sync::watch::Receiver<TapleSettings>,
        ) = tokio::sync::watch::channel(self.settings.clone());
        // Creation BBDD
        let db = self.database.take().unwrap();
        let db = Arc::new(db);
        let db_access = DB::new(db.clone());
        // Creation of cryptographic material
        let stored_public_key = db_access.get_controller_id().ok();
        let kp = self.generate_mc(stored_public_key)?;
        // Store controller_id in database
        db_access
            .set_controller_id(self.controller_id().unwrap())
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        let public_key = kp.public_key_bytes();
        let key_identifier = KeyIdentifier::new(kp.get_key_derivator(), &public_key);
        // Creation Network
        let network_manager = NetworkProcessor::new(
            Some(format!(
                "{}/{}",
                self.settings.network.addr.clone(),
                self.settings.network.p2p_port.clone()
            )),
            network_access_points(&self.settings.network.known_nodes)?, // TODO: Provide Bootraps nodes per configuration
            sender_network,
            kp.clone(),
            bsx.subscribe(),
            if self.settings.node.req_res {
                SendMode::RequestResponse
            } else {
                SendMode::Tell
            },
        )
        .await
        .expect("Error en creación de la capa de red");
        self.peer_id = Some(network_manager.local_peer_id().to_owned());
        // Creation NetworkReceiver
        let network_receiver =
            MessageReceiver::new(receiver_network, protocol_sender, bsx.subscribe());
        // Creation NetworkSender
        let network_sender = MessageSender::new(network_manager.client(), key_identifier.clone());
        // Creation Signature Manager
        let signature_manager = SelfSignatureManager::new(kp.clone(), &self.settings);
        // Creation TaskManager
        let mut task_manager = MessageTaskManager::new(
            network_sender.clone(),
            task_receiver,
            bsx.clone(),
            bsx.subscribe(),
        );
        // Creation ProtocolManager
        let protocol_manager = ProtocolManager::new(
            protocol_receiver,
            distribution_sender.clone(),
            evaluation_sender.clone(),
            validation_sender.clone(),
            event_sender.clone(),
            approval_sender.clone(),
            ledger_sender.clone(),
            bsx.clone(),
        );
        // Creation Governance
        let mut governance_manager = Governance::new(
            governance_receiver,
            bsx.clone(),
            bsx.subscribe(),
            DB::new(db.clone()),
            governance_update_sx.clone(),
        );
        // Creation EventManager
        let event_manager = EventManager::new(
            event_receiver,
            governance_update_rx,
            GovernanceAPI::new(governance_sender.clone()),
            DB::new(db.clone()),
            bsx.clone(),
            bsx.subscribe(),
            task_sender.clone(),
            self.notification_sender.clone(),
            ledger_sender,
            signature_manager.get_own_identifier(),
            signature_manager.clone(),
        );
        // Creation LedgerManager
        let ledger_manager = LedgerManager::new(
            ledger_receiver,
            bsx.clone(),
            bsx.subscribe(),
            self.notification_sender.clone(),
            GovernanceAPI::new(governance_sender.clone()),
            DB::new(db.clone()),
            task_sender.clone(),
            distribution_sender,
            key_identifier.clone(),
        );
        // Creation API module
        let api = API::new(
            self.api_input.take().unwrap(),
            EventAPI::new(event_sender),
            ApprovalAPI::new(approval_sender),
            wath_sender,
            self.settings.clone(),
            kp.clone(),
            bsx.clone(),
            bsx.subscribe(),
            DB::new(db.clone()),
        );
        // Creation EvaluatorManager
        let evaluator_manager = EvaluatorManager::new(
            evaluation_receiver,
            db.clone(),
            signature_manager.clone(),
            governance_update_sx.subscribe(),
            bsx.clone(),
            bsx.subscribe(),
            GovernanceAPI::new(governance_sender.clone()),
            self.settings.node.smartcontracts_directory.clone(),
            task_sender.clone(),
        );
        // Creation ApprovalManager
        let approval_manager = ApprovalManager::new(
            GovernanceAPI::new(governance_sender.clone()),
            approval_receiver,
            bsx.clone(),
            bsx.subscribe(),
            task_sender.clone(),
            governance_update_sx.subscribe(),
            signature_manager.clone(),
            self.notification_sender.clone(),
            self.settings.clone(),
            DB::new(db.clone()),
        );
        // Creation DistributionManager
        let distribution_manager = DistributionManager::new(
            distribution_receiver,
            governance_update_sx.subscribe(),
            bsx.clone(),
            bsx.subscribe(),
            task_sender.clone(),
            GovernanceAPI::new(governance_sender.clone()),
            signature_manager.clone(),
            self.settings.clone(),
            DB::new(db.clone()),
        );
        let validation_manager = NotaryManager::new(
            validation_receiver,
            GovernanceAPI::new(governance_sender),
            DB::new(db.clone()),
            signature_manager,
            bsx.clone(),
            bsx.subscribe(),
            task_sender,
        );
        // Module initialization
        tokio::spawn(async move {
            governance_manager.start().await;
        });
        tokio::spawn(async move {
            ledger_manager.start().await;
        });
        tokio::spawn(async move {
            event_manager.start().await;
        });
        tokio::spawn(async move {
            task_manager.start().await;
        });
        tokio::spawn(async move {
            protocol_manager.start().await;
        });
        tokio::spawn(async move {
            network_receiver.run().await;
        });
        tokio::spawn(async move {
            evaluator_manager.start().await;
        });
        tokio::spawn(async move {
            validation_manager.start().await;
        });
        tokio::spawn(async move {
            distribution_manager.start().await;
        });
        tokio::spawn(async move {
            approval_manager.start().await;
        });
        tokio::spawn(network_manager.run());
        // API Initialization
        tokio::spawn(async move {
            api.start().await;
        });
        Ok(())
    }

    fn create_key_pair(
        derivator: &KeyDerivator,
        seed: Option<String>,
        current_key: Option<String>,
    ) -> Result<KeyPair, Error> {
        let mut counter: u32 = 0;
        if seed.is_some() {
            counter += 1
        };
        if current_key.is_some() {
            counter += 2
        };
        if counter == 2 {
            let str_key = current_key.unwrap();
            match derivator {
                KeyDerivator::Ed25519 => Ok(KeyPair::Ed25519(Ed25519KeyPair::from_secret_key(
                    &hex::decode(str_key).map_err(|_| Error::InvalidHexString)?,
                ))),
                KeyDerivator::Secp256k1 => {
                    Ok(KeyPair::Secp256k1(Secp256k1KeyPair::from_secret_key(
                        &hex::decode(str_key).map_err(|_| Error::InvalidHexString)?,
                    )))
                }
            }
        } else if counter == 1 {
            match derivator {
                KeyDerivator::Ed25519 => Ok(KeyPair::Ed25519(
                    crate::commons::crypto::Ed25519KeyPair::from_seed(seed.unwrap().as_bytes()),
                )),
                KeyDerivator::Secp256k1 => Ok(KeyPair::Secp256k1(
                    crate::commons::crypto::Secp256k1KeyPair::from_seed(seed.unwrap().as_bytes()),
                )),
            }
        } else if counter == 3 {
            Err(Error::PkConflict)
        } else {
            Err(Error::NoMCAvailable)
        }
    }
}

fn check_dev_settings(settings: &TapleSettings) {
    if !settings.node.dev_mode {
        if settings.node.passvotation == 1 || settings.node.passvotation == 2 {
            log::error!("Invalid Settings for normal mode, try in dev mode");
            panic!("Invalid Settings for normal mode, try in dev mode")
        }
    }
}

fn network_access_points(points: &[String]) -> Result<Vec<(PeerId, Multiaddr)>, Error> {
    let mut access_points: Vec<(PeerId, Multiaddr)> = Vec::new();
    for point in points {
        let data: Vec<&str> = point.split("/p2p/").collect();
        if data.len() != 2 {
            return Err(Error::AcessPointError(point.to_string()));
        }
        if let Some(value) = multiaddr(point) {
            if let Ok(id) = data[1].parse::<PeerId>() {
                access_points.push((id, value));
            } else {
                return Err(Error::AcessPointError(format!(
                    "Invalid PeerId conversion: {}",
                    point
                )));
            }
        } else {
            return Err(Error::AcessPointError(format!(
                "Invalid MultiAddress conversion: {}",
                point
            )));
        }
    }
    Ok(access_points)
}

fn multiaddr(addr: &str) -> Option<Multiaddr> {
    match addr.parse::<Multiaddr>() {
        Ok(a) => Some(a),
        Err(_) => None,
    }
}
