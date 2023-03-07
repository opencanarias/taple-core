use crate::commons::{
    channel::{ChannelData, MpscChannel},
    identifier::KeyIdentifier,
};
use futures::future::{AbortHandle, Abortable, Aborted};
use log::debug;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use tokio::task::JoinHandle;

use super::algorithm::Algorithm;

use super::super::{
    error::Error, message_sender::MessageSender, MessageConfig, MessageTaskCommand,
    TaskCommandContent,
};

pub struct MessageTaskManager<T>
where
    T: TaskCommandContent + Serialize + DeserializeOwned,
{
    list: HashMap<String, (JoinHandle<Result<Result<(), Error>, Aborted>>, AbortHandle)>,
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
            list: HashMap::new(),
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
                crate::commons::channel::ChannelData::AskData(_) => {
                    panic!("Reciving Ask in MessageTaskManager")
                }
                crate::commons::channel::ChannelData::TellData(data) => data.get(),
            } {
                MessageTaskCommand::Request(id, message, targets, config) => match id {
                    Some(id) => {
                        self.create_indefinite_message_task(id, message, targets, config)
                            .await?;
                    }
                    None => self.create_message_task(message, targets, config)?,
                },
                MessageTaskCommand::Cancel(id) => {
                    self.cancel_task(&id).await?;
                }
            },
            None => {
                return Err(Error::SenderChannelError);
            }
        }
        Ok(())
    }

    async fn create_indefinite_message_task(
        &mut self,
        id: String,
        content: T,
        targets: Vec<KeyIdentifier>,
        config: MessageConfig,
    ) -> Result<(), Error> {
        if let Some(_entry) = self.list.get(&id) {
            self.cancel_task(&id).await?;
        }
        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        self.list.insert(
            id,
            (
                tokio::spawn(Abortable::new(
                    Algorithm::make_indefinite_future(
                        self.sender.clone(),
                        config,
                        content,
                        targets,
                    ),
                    abort_registration,
                )),
                abort_handle,
            ),
        );
        Ok(())
    }

    async fn cancel_task(&mut self, id: &String) -> Result<(), Error> {
        let Some((tokio_handler, abort_handler)) = self.list.remove(id) else {
            return Ok(())
        };
        abort_handler.abort();
        match tokio_handler.await {
            Err(error) => return Err(Error::TaskError { source: error }),
            Ok(inner_state) => match inner_state {
                Ok(task_result) => {
                    if let Err(e) = task_result {
                        debug!("Indefinite task did finish with error {:?}", e);
                    }
                }
                Err(_) => {
                    debug!("Task {} properly cancelled", id);
                }
            },
        };
        Ok(())
    }

    fn create_message_task(
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
}
