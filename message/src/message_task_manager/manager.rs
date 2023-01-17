use commons::{
    channel::{ChannelData, MpscChannel},
    identifier::KeyIdentifier,
};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;

use super::{algorithm::Algorithm, MessageTask};

use crate::{
    error::Error, message_sender::MessageSender, MessageConfig, MessageTaskCommand,
    TaskCommandContent,
};
use dashmap::DashMap;

pub struct MessageTaskManager<T>
where
    T: TaskCommandContent + Serialize + DeserializeOwned,
{
    list: Arc<DashMap<String, MessageTask<T>>>,
    receiver: MpscChannel<MessageTaskCommand<T>, ()>,
    sender: MessageSender,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl<T: TaskCommandContent + Serialize + DeserializeOwned + 'static> MessageTaskManager<T> {
    pub fn new(
        sender: MessageSender,
        receiver: MpscChannel<MessageTaskCommand<T>, ()>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    ) -> MessageTaskManager<T> {
        MessageTaskManager {
            list: Arc::new(DashMap::new()),
            receiver,
            sender,
            shutdown_sender,
            shutdown_receiver,
        }
    }

    pub async fn start(&mut self) {
        loop {
            tokio::select! {
                msg = self.receiver.receive() => {
                    let result = self.process_input(msg).await;
                    if result.is_err() {
                        self.shutdown_sender.send(()).expect("Shutdown Channel Closed");
                        break;
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
        input: Option<ChannelData<MessageTaskCommand<T>, ()>>,
    ) -> Result<(), Error> {
        match input {
            Some(data) => match match data {
                commons::channel::ChannelData::AskData(_) => {
                    panic!("Reciving Ask in MessageTaskManager")
                }
                commons::channel::ChannelData::TellData(data) => data.get(),
            } {
                MessageTaskCommand::Request(id, message, targets, config) => match id {
                    Some(id) => {
                        self.create_indefinite_message_task(id, message, targets, config)?
                    }
                    None => self.create_message_task(message, targets, config)?,
                },
                MessageTaskCommand::Cancel(id) => {
                    self.cancel_message_task(&id).await?;
                }
            },
            None => {
                return Err(Error::SenderChannelError);
            }
        }
        Ok(())
    }

    pub(crate) fn create_indefinite_message_task(
        &mut self,
        id: String,
        content: T,
        targets: Vec<KeyIdentifier>,
        config: MessageConfig,
    ) -> Result<(), Error> {
        if let Some(mut entry) = self.list.get_mut(&id) {
            entry.change_data(content);
        } else {
            self.list.insert(
                id.clone(),
                MessageTask::new(
                    content,
                    Algorithm::make_indefinite_future(
                        id,
                        self.list.clone(),
                        self.sender.clone(),
                        config,
                    ),
                    targets,
                ),
            );
        }
        Ok(())
    }

    pub(crate) fn create_message_task(
        &mut self,
        content: T,
        targets: Vec<KeyIdentifier>,
        config: MessageConfig,
    ) -> Result<(), Error> {
        tokio::spawn(Algorithm::make_future(
            content,
            targets,
            self.sender.clone(),
            config,
        ));
        Ok(())
    }

    pub(crate) async fn cancel_message_task(&self, id: &String) -> Result<(), Error> {
        let task = { self.list.remove(id) };
        if let Some((_, data)) = task {
            data.abort().await?;
            return Ok(());
        }
        Ok(())
    }
}
