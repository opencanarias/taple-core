use crate::{
    commons::channel::{ChannelData, MpscChannel},
    database::DB,
    governance::GovernanceAPI,
    DatabaseManager, Notification,
};

use super::{errors::LedgerError, ledger::Ledger, LedgerCommand, LedgerResponse};

pub struct EventManager<D: DatabaseManager> {
    /// Communication channel for incoming petitions
    input_channel: MpscChannel<LedgerCommand, LedgerResponse>,
    inner_ledger: Ledger<D>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    notification_sender: tokio::sync::broadcast::Sender<Notification>,
}

impl<D: DatabaseManager> EventManager<D> {
    pub fn new(
        input_channel: MpscChannel<LedgerCommand, LedgerResponse>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        notification_sender: tokio::sync::broadcast::Sender<Notification>,
        gov_api: GovernanceAPI,
        database: DB<D>,
    ) -> Self {
        Self {
            input_channel,
            inner_ledger: Ledger::new(gov_api, database),
            shutdown_receiver,
            shutdown_sender,
            notification_sender,
        }
    }

    pub async fn start(mut self) {
        match self.inner_ledger.init() {
            Ok(_) => {}
            Err(error) => {
                log::error!("Problemas con Init de Ledger Manager: {:?}", error);
                self.shutdown_sender.send(()).expect("Channel Closed");
                return;
            }
        };
        loop {
            tokio::select! {
                command = self.input_channel.receive() => {
                    match command {
                        Some(command) => {
                            let result = self.process_command(command).await;
                            if result.is_err() {
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                break;
                            }
                        }
                        None => {
                            self.shutdown_sender.send(()).expect("Channel Closed");
                            break;
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
        command: ChannelData<LedgerCommand, LedgerResponse>,
    ) -> Result<(), LedgerError> {
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
        let response = {
            match data {
                LedgerCommand::OwnEvent {
                    event,
                    signatures,
                } => todo!(),
                LedgerCommand::Genesis { event_request } => todo!(),
                LedgerCommand::ExternalEvent { event, signatures } => todo!(),
                LedgerCommand::ExternalIntermediateEvent { event } => todo!(),
            }
        };
        if sender.is_some() {
            sender.unwrap().send(response).expect("Sender Dropped");
        }
        Ok(())
    }
}
