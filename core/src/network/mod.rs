use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::message::{Command, NetworkEvent};

/// A trait representing the interaction between a TAPLE node and the network.
#[async_trait]
pub trait TapleNetwork: Send {
    fn client(&self) -> mpsc::Sender<Command>;
    async fn run(&mut self, sender: tokio::sync::mpsc::Sender<NetworkEvent>);
}
