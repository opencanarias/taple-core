#[derive(Debug, PartialEq)]
pub enum Command {
    StartProviding { keys: Vec<String> },
    SendMessage { receptor: Vec<u8>, message: Vec<u8> },
    Bootstrap,
}
