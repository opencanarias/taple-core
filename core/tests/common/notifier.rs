use std::str::FromStr;

use instant::Duration;
use taple_core::{DigestIdentifier, Notification, NotificationHandler};

use super::error::NotifierError;

const MAX_TIMEOUT_MS: u16 = 5000;

pub struct TapleNotifier {
    notifier: NotificationHandler,
}

impl TapleNotifier {
    pub fn new(notifier: NotificationHandler) -> Self {
        Self { notifier }
    }

    async fn wait_for_notification<V, F: Fn(Notification) -> Result<V, ()>>(
        &mut self,
        callback: F,
    ) -> Result<V, NotifierError> {
        let result = tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(MAX_TIMEOUT_MS as u64)) => {
                return Err(NotifierError::RequestTimeout);
            },
            subject_id = async {
                loop {
                    match self.notifier.receive().await {
                        Ok(data) => {
                          if let Ok(result) = callback(data) {
                            break Ok(result)
                          }
                        },
                        Err(_) => {
                          break Err(NotifierError::NotificationChannelClosed);
                        }
                    }
                }
            } => subject_id
        };
        result
    }

    pub async fn wait_for_new_subject(&mut self) -> Result<DigestIdentifier, NotifierError> {
        let subject_id = self
            .wait_for_notification(|data| {
                if let Notification::NewSubject { subject_id } = data {
                    Ok(subject_id)
                } else {
                    Err(())
                }
            })
            .await;
        Ok(DigestIdentifier::from_str(&subject_id?)
            .expect("Invalid conversion to digest identifier"))
    }

    pub async fn wait_for_new_event(&mut self) -> Result<(u64, DigestIdentifier), NotifierError> {
        let (sn, subject_id) = self
            .wait_for_notification(|data| {
                if let Notification::NewEvent { sn, subject_id } = data {
                    Ok((sn, subject_id))
                } else {
                    Err(())
                }
            })
            .await?;
        Ok((
            sn,
            DigestIdentifier::from_str(&subject_id)
                .expect("Invalid conversion to digest identifier"),
        ))
    }
}
