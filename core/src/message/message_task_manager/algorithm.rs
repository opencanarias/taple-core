use crate::commons::identifier::{Derivable, KeyIdentifier};
use futures::{future::BoxFuture, prelude::*};
use log::debug;

use super::super::{error::Error, MessageConfig, MessageSender, TaskCommandContent};
use std::time::Duration;

use rand::Rng;

pub struct Algorithm {}

impl Algorithm {
    fn get_targets(all_targets: Vec<KeyIdentifier>, replication_factor: f64) -> Vec<KeyIdentifier> {
        let number_to_select =
            1.max((all_targets.len() as f64 * replication_factor).floor() as u32);
        get_n_distinct_random_data(number_to_select, all_targets)
    }

    pub fn make_indefinite_future<T: 'static + TaskCommandContent>(
        sender: MessageSender,
        config: MessageConfig,
        request: T,
        targets: Vec<KeyIdentifier>,
    ) -> BoxFuture<'static, Result<(), Error>> {
        async move {
            let mut interval =
                tokio::time::interval(Duration::from_millis(config.timeout() as u64));
            loop {
                // The message to be sent is obtained
                interval.tick().await;
                // Targets are selected
                let targets_selected =
                    Algorithm::get_targets(targets.clone(), config.replication_factor());
                for target in targets_selected {
                    debug!("Message sent to {}", target.to_str());
                    sender
                        .send_message(target, request.clone())
                        .await
                        .map_err(|_| Error::SenderChannelError)?;
                }
            }
        }
        .boxed()
    }

    pub fn make_future<T: 'static + TaskCommandContent>(
        request: T,
        targets: Vec<KeyIdentifier>,
        sender: MessageSender,
        config: MessageConfig,
    ) -> BoxFuture<'static, Result<(), Error>> {
        async move {
            // Targets are selected
            let targets_selected = Algorithm::get_targets(targets, config.replication_factor());
            for target in targets_selected {
                let result_sending = sender.send_message(target, request.clone()).await;
                result_sending.map_err(|_| Error::SenderChannelError)?;
            }
            Ok(())
        }
        .boxed()
    }
}

fn get_n_distinct_random_data<D>(quantity: u32, mut data: Vec<D>) -> Vec<D> {
    if quantity as usize >= data.len() {
        return data;
    }
    let mut result: Vec<D> = Vec::new();
    let mut counter = 0u32;
    let mut rng = rand::thread_rng();
    while counter < quantity {
        let index = rng.gen_range(0..data.len());
        let value = data.remove(index);
        result.push(value);
        counter += 1;
    }
    result
}

#[cfg(test)]
mod test {

    use super::get_n_distinct_random_data;

    #[test]
    fn test_random_select() {
        let first = vec![1, 2, 3, 4, 5];
        let empty: Vec<i32> = vec![];
        assert_eq!(get_n_distinct_random_data(0, first.clone()), empty);
        assert_eq!(
            get_n_distinct_random_data(10, first.clone()),
            vec![1, 2, 3, 4, 5]
        );
        assert_eq!(get_n_distinct_random_data(3, first.clone()).len(), 3);
    }
}
