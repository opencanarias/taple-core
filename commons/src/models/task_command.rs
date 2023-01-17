use super::{message::{Message}, identifier::Identifier};

const DEFAULT_TIMEOUT_IN_MS: u32 = 10000;
const DEFAULT_REPLICATION_FACTOR: f32 = 0.25;
const DEFAULT_NUMBER_OF_RETRIES: u32 = 10;
pub struct MessageTaskRequestBuilder {
  message_id: Identifier,
  message: Message,
  targets: Vec<Vec<u8>>,
  timeout: u32,
  replication_factor: f32,
  number_of_retries: u32,
}

impl MessageTaskRequestBuilder {
  pub fn new(
    message_id: Identifier,
    message: Message,
    targets: Vec<Vec<u8>>
  ) -> Self {
    Self {
      message_id,
      message,
      targets,
      timeout: DEFAULT_TIMEOUT_IN_MS,
      replication_factor: DEFAULT_REPLICATION_FACTOR,
      number_of_retries: DEFAULT_NUMBER_OF_RETRIES,
    }
  }

  pub fn with_timeout(&mut self, timeout: u32) -> &mut Self {
    self.timeout = timeout;
    self
  }

  pub fn with_replication_factor(&mut self, replication_factor: f32) -> &mut Self {
    self.replication_factor = replication_factor;
    self
  }
  
  pub fn with_number_of_retries(&mut self, number_of_retries : u32) -> &mut Self{
    self.number_of_retries = number_of_retries;
    self
  }

  pub fn build(&self) -> MessageTaskRequest{
    MessageTaskRequest{
        message_id : self.message_id.clone(),
        message : self.message.clone(),
        targets : self.targets.clone(),
        config : TaskConfig { 
            timeout: self.timeout, 
            replication_factor: self.replication_factor, 
            number_of_retries: self.number_of_retries 
        }
    }
  }

}

#[derive(Clone, Debug)]
pub struct MessageTaskRequest {
  pub message_id: Identifier,
  pub message: Message,
  pub targets: Vec<Vec<u8>>,
  pub config: TaskConfig,
}

#[derive(Clone, Debug)]
pub struct TaskConfig {
  pub timeout: u32,
  pub replication_factor: f32,
  pub number_of_retries: u32,
}

impl TaskConfig {
  pub fn timeout(&self) -> u32 {
    self.timeout
  }

  pub fn replication_factor(&self) -> f32 {
    self.replication_factor
  }

  pub fn number_of_retries(&self) -> u32 {
    self.number_of_retries
  }
}
