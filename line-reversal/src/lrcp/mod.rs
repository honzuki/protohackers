use std::time::Duration;

const RETRANSMISSION_TIMEOUT: Duration = Duration::from_millis(100);
const SESSION_EXPIRY_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_MESSAGE_SIZE: usize = 1000;

// internal limitation to make sure we're within the max_message_size
const MAX_DATA_SIZE: usize = 910;

pub mod connection;
pub mod listener;
mod message;

pub use listener::Listener;
