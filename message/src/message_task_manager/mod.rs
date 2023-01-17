mod algorithm;
mod manager;
use crate::{error::Error, TaskCommandContent};
use commons::identifier::KeyIdentifier;
use futures::future::BoxFuture;
use tokio::task::JoinHandle;

pub use manager::MessageTaskManager;

pub struct MessageTask<T: TaskCommandContent> {
    handler: JoinHandle<Result<(), Error>>,
    request_data: T,
    targets: Vec<KeyIdentifier>,
}

impl<T: TaskCommandContent> MessageTask<T> {
    pub fn new(
        request_data: T,
        future: BoxFuture<'static, Result<(), Error>>,
        targets: Vec<KeyIdentifier>,
    ) -> Self {
        let handler = tokio::spawn(future);
        MessageTask {
            handler,
            request_data,
            targets,
        }
    }

    pub async fn abort(self) -> Result<(), Error> {
        self.handler.abort();
        match self.handler.await {
            Ok(_) => Ok(()),
            Err(error) => {
                if error.is_cancelled() {
                    Ok(())
                } else {
                    Err(Error::TaskError { source: error })
                }
            }
        }
    }

    pub fn change_data(&mut self, request_data: T) {
        self.request_data = request_data;
    }

    pub fn get_data(&self) -> T {
        self.request_data.clone()
    }

    pub fn get_targets(&self) -> Vec<KeyIdentifier> {
        self.targets.clone()
    }
}
