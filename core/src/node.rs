#[cfg(feature = "approval")]
use crate::approval::manager::{ApprovalAPI, ApprovalManager};
#[cfg(feature = "approval")]
use crate::approval::{ApprovalMessages, ApprovalResponses};
use crate::authorized_subjecs::manager::{AuthorizedSubjectsAPI, AuthorizedSubjectsManager};
use crate::authorized_subjecs::{AuthorizedSubjectsCommand, AuthorizedSubjectsResponse};
use crate::commons::channel::MpscChannel;
use crate::commons::crypto::{KeyMaterial, KeyPair};
use crate::commons::identifier::derive::KeyDerivator;
use crate::commons::identifier::{Derivable, KeyIdentifier};
use crate::commons::models::notification::Notification;
use crate::commons::self_signature_manager::{SelfSignatureInterface, SelfSignatureManager};
use crate::commons::settings::Settings;
use crate::database::{DatabaseCollection, DatabaseManager, DB};
use crate::distribution::error::DistributionErrorResponses;
use crate::distribution::manager::DistributionManager;
use crate::distribution::DistributionMessagesNew;
#[cfg(feature = "evaluation")]
use crate::evaluator::{EvaluatorManager, EvaluatorMessage, EvaluatorResponse};
use crate::event::manager::{EventAPI, EventManager};
use crate::event::{EventCommand, EventResponse};
use crate::governance::GovernanceAPI;
use crate::governance::{governance::Governance, GovernanceMessage, GovernanceResponse};
use crate::ledger::manager::EventManagerAPI;
use crate::ledger::{manager::LedgerManager, LedgerCommand, LedgerResponse};
use crate::message::{
    MessageContent, MessageReceiver, MessageSender, MessageTaskCommand, MessageTaskManager,
    NetworkEvent,
};
use crate::network::network::NetworkProcessor;
use crate::protocol::protocol_message_manager::{ProtocolManager, TapleMessages};
use crate::signature::Signed;
#[cfg(feature = "validation")]
use crate::validation::manager::ValidationManager;
#[cfg(feature = "validation")]
use crate::validation::{ValidationCommand, ValidationResponse};
use ::futures::Future;
use libp2p::{Multiaddr, PeerId};
use log::{error, info};
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::*;
use tokio_util::sync::CancellationToken;

use crate::api::{Api, ApiManager};
use crate::error::Error;

const BUFFER_SIZE: usize = 1000;

/// Structure representing a TAPLE node
///
/// A node must be instantiated using the [`Taple::build`] method, which requires a set
/// of [configuration](Settings) parameters in order to be properly initialized.
///
#[derive(Debug)]
pub struct Node<M: DatabaseManager<C>, C: DatabaseCollection> {
    notification_rx: mpsc::Receiver<Notification>,
    token: CancellationToken,
    _m: PhantomData<M>,
    _c: PhantomData<C>,
}

impl<M: DatabaseManager<C> + 'static, C: DatabaseCollection + 'static> Node<M, C> {
    /// This method creates and initializes a TAPLE node.
    /// # Possible results
    /// If the process is successful, the method will return `Ok(())`.
    /// An error will be returned only if it has not been possible to generate the necessary data
    /// for the initialization of the components, mainly due to problems in the initial [configuration](Settings).
    /// # Panics
    /// This method panics if it has not been possible to generate the network layer.
    pub fn build(settings: Settings, database: M) -> Result<(Self, Api), Error> {
        let (api_rx, api_tx) = MpscChannel::new(BUFFER_SIZE);

        let (notification_tx, notification_rx) = mpsc::channel(BUFFER_SIZE);

        let (network_tx, network_rx): (mpsc::Sender<NetworkEvent>, mpsc::Receiver<NetworkEvent>) =
            mpsc::channel(BUFFER_SIZE);

        let (event_rx, event_tx) = MpscChannel::<EventCommand, EventResponse>::new(BUFFER_SIZE);

        let (ledger_rx, ledger_tx) = MpscChannel::<LedgerCommand, LedgerResponse>::new(BUFFER_SIZE);

        let (as_rx, as_tx) =
            MpscChannel::<AuthorizedSubjectsCommand, AuthorizedSubjectsResponse>::new(BUFFER_SIZE);

        let (governance_rx, governance_tx) =
            MpscChannel::<GovernanceMessage, GovernanceResponse>::new(BUFFER_SIZE);

        // TODO: broadcast channel. Is a lag corretly managed?
        let (governance_update_sx, governance_update_rx) = broadcast::channel(BUFFER_SIZE);

        let (task_rx, task_tx) =
            MpscChannel::<MessageTaskCommand<TapleMessages>, ()>::new(BUFFER_SIZE);

        let (protocol_rx, protocol_tx) =
            MpscChannel::<Signed<MessageContent<TapleMessages>>, ()>::new(BUFFER_SIZE);

        let (distribution_rx, distribution_tx) = MpscChannel::<
            DistributionMessagesNew,
            Result<(), DistributionErrorResponses>,
        >::new(BUFFER_SIZE);

        #[cfg(feature = "approval")]
        let (approval_rx, approval_tx) =
            MpscChannel::<ApprovalMessages, ApprovalResponses>::new(BUFFER_SIZE);

        #[cfg(feature = "evaluation")]
        let (evaluation_rx, evaluation_tx) =
            MpscChannel::<EvaluatorMessage, EvaluatorResponse>::new(BUFFER_SIZE);

        #[cfg(feature = "validation")]
        let (validation_rx, validation_tx) =
            MpscChannel::<ValidationCommand, ValidationResponse>::new(BUFFER_SIZE);

        let database = Arc::new(database);

        let kp = Self::register_node_key(
            &settings.node.key_derivator,
            &settings.node.secret_key,
            DB::new(database.clone()),
        )?;

        let controller_id = KeyIdentifier::new(kp.get_key_derivator(), &kp.public_key_bytes());
        info!("Controller ID: {}", &controller_id);

        let token = CancellationToken::new();

        let network_manager = NetworkProcessor::new(
            settings.network.listen_addr.clone(),
            network_access_points(&settings.network.known_nodes)?,
            network_tx,
            kp.clone(),
            token.clone(),
            notification_tx.clone(),
            external_addresses(&settings.network.external_address)?,
        )
        .expect("Network created");

        //TODO: change name. It's not a task
        let signature_manager = SelfSignatureManager::new(kp.clone(), &settings);

        //TODO: change name. It's a task
        let network_rx = MessageReceiver::new(
            network_rx,
            protocol_tx,
            token.clone(),
            notification_tx.clone(),
            signature_manager.get_own_identifier(),
        );

        let network_tx = MessageSender::new(
            network_manager.client(),
            controller_id.clone(),
            signature_manager.clone(),
        );

        let task_manager =
            MessageTaskManager::new(network_tx, task_rx, token.clone(), notification_tx.clone());

        let protocol_manager = ProtocolManager::new(
            protocol_rx,
            distribution_tx.clone(),
            #[cfg(feature = "evaluation")]
            evaluation_tx,
            #[cfg(feature = "validation")]
            validation_tx,
            event_tx.clone(),
            #[cfg(feature = "approval")]
            approval_tx.clone(),
            ledger_tx.clone(),
            token.clone(),
            notification_tx.clone(),
        );

        let mut governance_manager = Governance::<M, C>::new(
            governance_rx,
            token.clone(),
            notification_tx.clone(),
            DB::new(database.clone()),
            governance_update_sx.clone(),
        );

        let event_manager = EventManager::new(
            event_rx,
            governance_update_rx,
            GovernanceAPI::new(governance_tx.clone()),
            DB::new(database.clone()),
            token.clone(),
            task_tx.clone(),
            notification_tx.clone(),
            ledger_tx.clone(),
            signature_manager.get_own_identifier(),
            signature_manager.clone(),
        );

        let ledger_manager = LedgerManager::new(
            ledger_rx,
            token.clone(),
            notification_tx.clone(),
            GovernanceAPI::new(governance_tx.clone()),
            DB::new(database.clone()),
            task_tx.clone(),
            distribution_tx,
            controller_id.clone(),
        );

        let as_manager = AuthorizedSubjectsManager::new(
            as_rx,
            DB::new(database.clone()),
            task_tx.clone(),
            controller_id.clone(),
            token.clone(),
            notification_tx.clone(),
        );

        let api_manager = ApiManager::new(
            api_rx,
            EventAPI::new(event_tx),
            #[cfg(feature = "approval")]
            ApprovalAPI::new(approval_tx),
            AuthorizedSubjectsAPI::new(as_tx),
            EventManagerAPI::new(ledger_tx),
            token.clone(),
            notification_tx.clone(),
            DB::new(database.clone()),
        );

        #[cfg(feature = "evaluation")]
        let evaluator_manager = EvaluatorManager::new(
            evaluation_rx,
            database.clone(),
            signature_manager.clone(),
            governance_update_sx.subscribe(),
            token.clone(),
            notification_tx.clone(),
            GovernanceAPI::new(governance_tx.clone()),
            settings.node.smartcontracts_directory.clone(),
            task_tx.clone(),
        );

        #[cfg(feature = "approval")]
        let approval_manager = ApprovalManager::new(
            GovernanceAPI::new(governance_tx.clone()),
            approval_rx,
            token.clone(),
            task_tx.clone(),
            governance_update_sx.subscribe(),
            signature_manager.clone(),
            notification_tx.clone(),
            settings.clone(),
            DB::new(database.clone()),
        );

        let distribution_manager = DistributionManager::new(
            distribution_rx,
            governance_update_sx.subscribe(),
            token.clone(),
            notification_tx.clone(),
            task_tx.clone(),
            GovernanceAPI::new(governance_tx.clone()),
            signature_manager.clone(),
            settings,
            DB::new(database.clone()),
        );

        #[cfg(feature = "validation")]
        let validation_manager = ValidationManager::new(
            validation_rx,
            GovernanceAPI::new(governance_tx),
            DB::new(database),
            signature_manager,
            token.clone(),
            notification_tx,
            task_tx,
        );

        let taple = Node {
            notification_rx,
            token,
            _m: PhantomData::default(),
            _c: PhantomData::default(),
        };

        let api = Api::new(
            network_manager.local_peer_id().to_owned(),
            controller_id.to_str(),
            kp.public_key_bytes(),
            api_tx,
        );

        tokio::spawn(async move {
            governance_manager.run().await;
        });

        tokio::spawn(async move {
            ledger_manager.run().await;
        });

        tokio::spawn(async move {
            event_manager.run().await;
        });

        tokio::spawn(async move {
            task_manager.run().await;
        });

        tokio::spawn(async move {
            protocol_manager.run().await;
        });

        tokio::spawn(async move {
            network_rx.run().await;
        });

        #[cfg(feature = "evaluation")]
        tokio::spawn(async move {
            evaluator_manager.run().await;
        });

        #[cfg(feature = "validation")]
        tokio::spawn(async move {
            validation_manager.run().await;
        });

        tokio::spawn(async move {
            distribution_manager.run().await;
        });

        #[cfg(feature = "approval")]
        tokio::spawn(async move {
            approval_manager.run().await;
        });

        tokio::spawn(async move {
            as_manager.run().await;
        });

        tokio::spawn(async move {
            network_manager.run().await;
        });

        tokio::spawn(async move {
            api_manager.run().await;
        });

        Ok((taple, api))
    }

    pub async fn recv_notification(&mut self) -> Option<Notification> {
        self.notification_rx.recv().await
    }

    pub async fn handle_notifications<H>(mut self, handler: H)
    where
        H: Fn(Notification),
    {
        while let Some(notification) = self.recv_notification().await {
            handler(notification);
        }
    }

    pub async fn drop_notifications(self) {
        self.handle_notifications(|_| {}).await;
    }

    /// Bind the node with a shutdown signal.
    ///
    /// When the signal completes, the server will start the graceful shutdown
    /// process. The node can be bind to multiple signals.
    pub fn bind_with_shutdown(&self, signal: impl Future<Output = ()> + Send + 'static) {
        let token = self.token.clone();
        tokio::spawn(async move {
            signal.await;
            token.cancel();
        });
    }

    pub async fn shutdown_gracefully(self) {
        self.token.cancel();
        self.drop_notifications().await;
    }

    fn register_node_key(
        key_derivator: &KeyDerivator,
        secret_key: &str,
        db: DB<C>,
    ) -> Result<KeyPair, Error> {
        let key = KeyPair::from_hex(key_derivator, secret_key)
            .map_err(|_| Error::InvalidHexString)
            .unwrap();
        let identifier =
            KeyIdentifier::new(key.get_key_derivator(), &key.public_key_bytes()).to_str();
        let stored_identifier = db.get_controller_id().ok();
        if let Some(stored_identifier) = stored_identifier {
            if identifier != stored_identifier {
                error!("Invalid key. There is a differente key stored");
                return Err(Error::InvalidKeyPairSpecified(stored_identifier));
            }
        } else {
            db.set_controller_id(identifier)
                .map_err(|e| Error::DatabaseError(e.to_string()))?;
        }
        Ok(key)
    }
}

// TODO: move to better place, maybe settings
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

// TODO: move to better place, maybe settings
fn external_addresses(addresses: &[String]) -> Result<Vec<Multiaddr>, Error> {
    let mut external_addresses: Vec<Multiaddr> = Vec::new();
    for address in addresses {
        if let Some(value) = multiaddr(address) {
            external_addresses.push(value);
        } else {
            return Err(Error::AcessPointError(format!(
                "Invalid MultiAddress conversion in External Address: {}",
                address
            )));
        }
    }
    Ok(external_addresses)
}

// TODO: move to better place, maybe settings
fn multiaddr(addr: &str) -> Option<Multiaddr> {
    match addr.parse::<Multiaddr>() {
        Ok(a) => Some(a),
        Err(_) => None,
    }
}
