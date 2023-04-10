pub mod manager;
pub mod event_completer;
pub mod errors;

#[derive(Debug, Clone)]
pub enum EventCommand {
    // NotaryEvent(NotaryEvent),
}

#[derive(Debug, Clone)]
pub enum EventResponse {
    // NotaryEventResponse(Result<NotaryEventResponse, NotaryError>),
}