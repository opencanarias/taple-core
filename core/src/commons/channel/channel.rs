use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    oneshot::{self, Sender as OneshotSender},
};

use crate::commons::errors;

/// An enum representing the data that can be sent over a channel.
#[derive(Debug)]
pub enum ChannelData<I, R>
where
    I: Send,
    R: Send,
{
    /// Data sent as a request for information.
    AskData(AskData<I, R>),
    /// Data sent as a notification or update.
    TellData(TellData<I>),
}

/// A struct representing a request for information sent over a channel.
#[derive(Debug)]
pub struct AskData<I, R>
where
    I: Send,
    R: Send,
{
    /// The sender for the response to the request.
    sender: OneshotSender<R>,
    /// The data being requested.
    data: I,
}

impl<I: Send, R: Send> AskData<I, R> {
    /// Consumes the `AskData` and returns a tuple containing the sender for the response and the requested data.
    pub fn get(self) -> (OneshotSender<R>, I) {
        (self.sender, self.data)
    }

    /// Sends a response to the request.
    ///
    /// # Arguments
    ///
    /// * `data` - The response data to send.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection to the sender is closed.
    #[allow(dead_code)]
    pub fn send_response(self, data: R) -> Result<(), String> {
        self.sender
            .send(data)
            .map_err(|_| "Connection Closed".to_owned())
    }
}

/// A struct representing a notification or update sent over a channel.
#[derive(Debug)]
pub struct TellData<I>
where
    I: Send,
{
    /// The data being sent.
    data: I,
}

impl<I: Send> TellData<I> {
    /// Consumes the `TellData` and returns the data being sent.
    pub fn get(self) -> I {
        self.data
    }
}

#[derive(Clone, Debug)]
pub struct SenderEnd<I, R>
where
    I: Send,
    R: Send,
{
    /// The sender for the channel.
    sender: Sender<ChannelData<I, R>>,
}

impl<I: Send, R: Send> SenderEnd<I, R> {
    /// Creates a new `SenderEnd` with the given `Sender`.
    fn new(end: Sender<ChannelData<I, R>>) -> SenderEnd<I, R> {
        SenderEnd { sender: end }
    }

    /// Sends a request for information over the channel and waits for a response.
    ///
    /// # Arguments
    ///
    /// * `data` - The data to send as the request.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the response to the request, or an error if the channel is closed.
    pub async fn ask(&self, data: I) -> Result<R, errors::ChannelErrors> {
        // Create the oneshot channels
        let (sx, rx) = oneshot::channel::<R>();
        // Send the data
        self.sender
            .send(ChannelData::AskData(AskData { sender: sx, data }))
            .await
            .map_err(|_| errors::ChannelErrors::ChannelClosed)?;
        // The other side will process the data and we are waiting for your response:
        let result = rx.await.map_err(|_| errors::ChannelErrors::ChannelClosed)?;
        // Return the answer
        Ok(result)
    }

    #[allow(dead_code)]
    pub fn try_tell(&self, data: I) -> Result<(), errors::ChannelErrors> {
        if let Ok(permit) = self.sender.try_reserve() {
            Ok(permit.send(ChannelData::TellData(TellData { data: data })))
        } else {
            Err(errors::ChannelErrors::FullQueue)
        }
    }

    /// Sends a notification or update over the channel.
    ///
    /// # Arguments
    ///
    /// * `data` - The data to send as the notification or update.
    pub async fn tell(&self, data: I) -> Result<(), errors::ChannelErrors> {
        self.sender
            .send(ChannelData::TellData(TellData { data: data }))
            .await
            .map_err(|_| errors::ChannelErrors::ChannelClosed)
    }
}

/// A struct representing a multi-producer, single-consumer channel.
#[derive(Debug)]
pub struct MpscChannel<I, R>
where
    I: Send,
    R: Send,
{
    /// The receiver end of the channel.
    receiver: Receiver<ChannelData<I, R>>,
}

impl<I: Send, R: Send> MpscChannel<I, R> {
    /// Creates a new `MpscChannel` with the given buffer size and returns a tuple containing the channel and its sender end.
    ///
    /// # Arguments
    ///
    /// * `buffer` - The size of the buffer for the channel.
    ///
    /// # Returns
    ///
    /// Returns a tuple containing the `MpscChannel` and its sender end.
    pub fn new(buffer: usize) -> (Self, SenderEnd<I, R>) {
        let (sender, receiver) = mpsc::channel::<ChannelData<I, R>>(buffer);
        (Self { receiver }, SenderEnd::new(sender))
    }

    /// Receives a message from the channel.
    ///
    /// # Returns
    ///
    /// Returns an `Option` containing the received message, or `None` if the channel is closed.
    pub async fn receive(&mut self) -> Option<ChannelData<I, R>> {
        self.receiver.recv().await
    }
}

#[cfg(test)]
mod test {
    use super::{ChannelData, MpscChannel};
    use tokio::runtime::Runtime;

    struct Processor {}
    impl Processor {
        async fn process_ask(data: u32, sender: tokio::sync::oneshot::Sender<String>) {
            sender.send(format!("{} Sent", data)).unwrap();
        }
        fn process_tell(data: u32) {
            println!("Recibido --> {}", data);
        }
    }

    #[test]
    fn test_only_ask() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut channel, sender) = MpscChannel::<u32, String>::new(100);
            tokio::spawn(async move {
                let (sender, data) =
                    if let ChannelData::AskData(data) = channel.receive().await.unwrap() {
                        data.get()
                    } else {
                        panic!("Unexpected");
                    };
                assert_eq!(10, data);
                Processor::process_ask(data, sender).await;
                let (sender, data) =
                    if let ChannelData::AskData(data) = channel.receive().await.unwrap() {
                        data.get()
                    } else {
                        panic!("Unexpected");
                    };
                assert_eq!(777, data);
                Processor::process_ask(data, sender).await;
            });
            let result = sender.ask(10).await.unwrap();
            assert_eq!(result, "10 Sent".to_owned());
            let result = sender.ask(777).await.unwrap();
            assert_eq!(result, "777 Sent".to_owned());
            return;
        });
    }

    #[test]
    fn test_only_try_tell() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut channel, sender) = MpscChannel::<u32, String>::new(100);
            tokio::spawn(async move {
                let data = if let ChannelData::TellData(data) = channel.receive().await.unwrap() {
                    data.get()
                } else {
                    panic!("Unexpected");
                };
                Processor::process_tell(data);
                let data = if let ChannelData::TellData(data) = channel.receive().await.unwrap() {
                    data.get()
                } else {
                    panic!("Unexpected");
                };
                Processor::process_tell(data);
            });
            let result = sender.try_tell(10);
            assert!(result.is_ok());
            let result = sender.try_tell(777);
            assert!(result.is_ok());
            return;
        });
    }

    #[test]
    fn test_only_tell() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut channel, sender) = MpscChannel::<u32, String>::new(100);
            tokio::spawn(async move {
                let data = if let ChannelData::TellData(data) = channel.receive().await.unwrap() {
                    data.get()
                } else {
                    panic!("Unexpected");
                };
                Processor::process_tell(data);
                let data = if let ChannelData::TellData(data) = channel.receive().await.unwrap() {
                    data.get()
                } else {
                    panic!("Unexpected");
                };
                Processor::process_tell(data);
            });
            let result = sender.tell(10).await;
            assert!(result.is_ok());
            let result = sender.tell(777).await;
            assert!(result.is_ok());
            return;
        });
    }

    #[test]
    fn test_tell_and_ask() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut channel, sender) = MpscChannel::<u32, String>::new(100);
            tokio::spawn(async move {
                let (sender, data) =
                    if let ChannelData::AskData(data) = channel.receive().await.unwrap() {
                        data.get()
                    } else {
                        panic!("Unexpected");
                    };
                assert_eq!(10, data);
                Processor::process_ask(data, sender).await;
                let data = if let ChannelData::TellData(data) = channel.receive().await.unwrap() {
                    data.get()
                } else {
                    panic!("Unexpected");
                };
                Processor::process_tell(data);
            });
            let result = sender.ask(10).await.unwrap();
            assert_eq!(result, "10 Sent".to_owned());
            let result = sender.try_tell(777);
            assert!(result.is_ok());
            return;
        });
    }
}
