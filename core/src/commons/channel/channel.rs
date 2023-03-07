use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    oneshot::{self, Sender as OneshotSender},
};

use crate::commons::errors;

#[derive(Debug)]
pub enum ChannelData<I, R>
where
    I: Send,
    R: Send,
{
    AskData(AskData<I, R>),
    TellData(TellData<I>),
}

#[derive(Debug)]
pub struct AskData<I, R>
where
    I: Send,
    R: Send,
{
    sender: OneshotSender<R>,
    data: I,
}
impl<I: Send, R: Send> AskData<I, R> {
    pub fn get(self) -> (OneshotSender<R>, I) {
        (self.sender, self.data)
    }

    #[allow(dead_code)]
    pub fn send_response(self, data: R) -> Result<(), String> {
        self.sender
            .send(data)
            .map_err(|_| "Connection Closed".to_owned())
    }
}

#[derive(Debug)]
pub struct TellData<I>
where
    I: Send,
{
    data: I,
}
impl<I: Send> TellData<I> {
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
    sender: Sender<ChannelData<I, R>>,
}

impl<I: Send, R: Send> SenderEnd<I, R> {
    fn new(end: Sender<ChannelData<I, R>>) -> SenderEnd<I, R> {
        SenderEnd { sender: end }
    }
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

    pub async fn tell(&self, data: I) -> Result<(), errors::ChannelErrors> {
        self.sender
            .send(ChannelData::TellData(TellData { data: data }))
            .await
            .map_err(|_| errors::ChannelErrors::ChannelClosed)
    }
}

#[derive(Debug)]
pub struct MpscChannel<I, R>
where
    I: Send,
    R: Send,
{
    receiver: Receiver<ChannelData<I, R>>,
}

impl<I: Send, R: Send> MpscChannel<I, R> {
    pub fn new(buffer: usize) -> (Self, SenderEnd<I, R>) {
        let (sx, _rx) = mpsc::channel::<ChannelData<I, R>>(buffer);
        (Self { receiver: _rx }, SenderEnd::new(sx))
    }

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
