use crate::{
  commons::{
      channel::{ChannelData, MpscChannel, SenderEnd},
  },
  governance::GovernanceAPI,
  protocol::command_head_manager::self_signature_manager::SelfSignatureManager,
};
use crate::database::{DB, DatabaseManager};
use super::{EvaluatorMessage, EvaluatorResponse, errors::EvaluatorError};

#[derive(Clone, Debug)]
pub struct EvaluatorAPI {
  sender: SenderEnd<EvaluatorMessage, EvaluatorResponse>,
}

impl EvaluatorAPI {
  pub fn new(sender: SenderEnd<EvaluatorMessage, EvaluatorResponse>) -> Self {
      Self { sender }
  }
}

pub struct EvaluatorManager<D: DatabaseManager> {
  /// Communication channel for incoming petitions
  input_channel: MpscChannel<EvaluatorMessage, EvaluatorResponse>,
  /// Notarization functions
  inner_notary: Notary<D>,
  // TODO: Añadir módulo compilación
  shutdown_sender: tokio::sync::broadcast::Sender<()>,
  shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl<D: DatabaseManager> EvaluatorManager<D> {
  pub fn new(
      input_channel: MpscChannel<EvaluatorMessage, EvaluatorResponse>,
      gov_api: GovernanceAPI,
      database: DB<D>,
      signature_manager: SelfSignatureManager,
      shutdown_sender: tokio::sync::broadcast::Sender<()>,
      shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
  ) -> Self {
      Self {
          input_channel,
          inner_notary: Notary::new(gov_api, database, signature_manager),
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
  ) -> Result<(), NotaryError> {
      // let (sender, data) = match command {
      //     ChannelData::AskData(data) => {
      //         let (sender, data) = data.get();
      //         (Some(sender), data)
      //     }
      //     ChannelData::TellData(data) => {
      //         let data = data.get();
      //         (None, data)
      //     }
      // };
      // let response = {
      //     match data {
      //         EvaluatorMessage::NotaryEvent(notary_event) => {
      //             let result = self.inner_notary.notary_event(notary_event).await;
      //             match result {
      //                 Err(NotaryError::ChannelError(_)) => return result.map(|_| ()),
      //                 _ => EvaluatorResponse::NotaryEventResponse(result),
      //             }
      //         }
      //     }
      // };
      // if sender.is_some() {
      //     sender.unwrap().send(response).expect("Sender Dropped");
      // }
      Ok(())
  }
}
