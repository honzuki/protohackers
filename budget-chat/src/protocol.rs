use tokio::sync::{mpsc, oneshot};

// back pressure measurements
pub const MESSAGE_BUFFER_COUNT: usize = 100;

pub const SYSTEM_MESSAGE_PREFIX: char = '*';
pub const MAX_USERNAME_SIZE: usize = 16;
pub const MAX_MESSAGE_SIZE: usize = 1000;

pub struct Join {
    pub username: String,
    pub response: oneshot::Sender<Result<JoinSuccess, JoinError>>,
}

pub struct JoinSuccess {
    pub userlist: Vec<String>,
    pub rx: FromChatRoom,
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum JoinError {
    #[error("The username \"{0}\" is already in use!")]
    BadUsername(String),
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub from: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Leave {
    pub username: String,
}

pub enum ToChatRoomMessage {
    Join(Join),
    ChatMessage(ChatMessage),
    Leave(Leave),
}

pub struct FromChatRoom {
    pub receiver: mpsc::Receiver<FromChatRoomMessage>,
}

#[derive(Debug, Clone)]
pub enum FromChatRoomMessage {
    Join(String),
    Leave(String),
    // Username , Message
    ChatMessage(String, String),
}
