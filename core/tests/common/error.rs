use thiserror::Error;

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum NotifierError {
    #[error("Petition timeout")]
    RequestTimeout,
    #[error("Notification channel closed")]
    NotificationChannelClosed,
}

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum TapleError {
    #[error("Taple Start Error {0}")]
    StartError(taple_core::Error),
}
