use crate::{
    commons::{
        bd::db::DB,
        channel::{ChannelData, MpscChannel, SenderEnd},
    },
    governance::GovernanceAPI, protocol::command_head_manager::self_signature_manager::SelfSignatureManager,
};

use super::{errors::NotaryError, notary::Notary, NotaryCommand, NotaryResponse};

#[derive(Clone, Debug)]
pub struct NotaryAPI {
    sender: SenderEnd<NotaryCommand, NotaryResponse>,
}

impl NotaryAPI {
    pub fn new(sender: SenderEnd<NotaryCommand, NotaryResponse>) -> Self {
        Self { sender }
    }
}

pub struct NotaryManager {
    /// Communication channel for incoming petitions
    input_channel: MpscChannel<NotaryCommand, NotaryResponse>,
    /// Notarization functions
    inner_notary: Notary,
}

impl NotaryManager {
    pub fn new(
        input_channel: MpscChannel<NotaryCommand, NotaryResponse>,
        gov_api: GovernanceAPI,
        database: DB,
        signature_manager: SelfSignatureManager,
    ) -> Self {
        Self {
            input_channel,
            inner_notary: Notary::new(gov_api, database, signature_manager),
        }
    }

    pub async fn start(mut self) -> Result<(), NotaryError> {
        loop {
            tokio::select! {
                command = self.input_channel.receive() => {
                    match command {
                        Some(command) => match self.process_command(command).await {
                            Ok(_) => {
                            },
                            Err(error) => {
                                match error {
                                    _ => todo!()
                                }
                            },
                        }
                        None => {
                            log::error!("Pete");
                            return Err(NotaryError::InputChannelError)
                        },
                    }
                }
            }
        }
    }

    async fn process_command(
        &mut self,
        command: ChannelData<NotaryCommand, NotaryResponse>,
    ) -> Result<(), NotaryError> {
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
                NotaryCommand::NotaryEvent(notary_event) => todo!(),
            }
        };
        if sender.is_some() {
            sender.unwrap().send(response).expect("Sender Dropped");
        }
        Ok(())
    }
}
