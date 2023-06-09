use serde::{Serialize, Deserialize};

use crate::{DigestIdentifier, event_request::EventRequest};

#[derive(
  Debug, Clone, Serialize, Deserialize,
)]
pub struct TapleRequest {
  pub id: DigestIdentifier,
  pub subject_id: Option<DigestIdentifier>,
  pub sn: Option<u64>,
  pub event_request: EventRequest,
  pub state: RequestState
}

#[derive(
  Debug, Clone, Serialize, Deserialize,
)]
pub enum RequestState {
  Finished,
  Error,
  Processing
}
