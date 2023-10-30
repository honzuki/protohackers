const SPEED_FACTOR: u16 = 100;

pub mod message_type {
    pub const ERROR: u8 = 0x10;
    pub const PLATE: u8 = 0x20;
    pub const TICKET: u8 = 0x21;
    pub const WANT_HEARTBEAT: u8 = 0x40;
    pub const HEARTBEAT: u8 = 0x41;
    pub const I_AM_CAMERA: u8 = 0x80;
    pub const I_AM_DISPATCHER: u8 = 0x81;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FromClient {
    Plate { plate: String, timestamp: u32 },
    WantHeartbeat { interval: u32 },
    IAmCamera { road: u16, mile: u16, limit: u16 },
    IAmDispatcher { roads: Vec<u16> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ToClientInternal {
    Error {
        msg: String,
    },
    Ticket {
        plate: String,
        road: u16,
        first_record: (u16, u32),
        second_record: (u16, u32),
        speed: u16,
    },
    Heartbeat,
}

// Hide the internal ToClient enum to provide a cleaner interface to the user
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToClient {
    pub(super) internal: ToClientInternal,
}

impl ToClient {
    pub fn error(reason: String) -> Self {
        Self {
            internal: ToClientInternal::Error { msg: reason },
        }
    }

    pub fn ticket(
        plate: String,
        road: u16,
        first_record: (u16, u32),
        second_record: (u16, u32),
        speed: u16,
    ) -> Self {
        Self {
            internal: ToClientInternal::Ticket {
                plate,
                road,
                first_record,
                second_record,
                speed: speed * SPEED_FACTOR,
            },
        }
    }

    pub fn heartbeat() -> Self {
        Self {
            internal: ToClientInternal::Heartbeat,
        }
    }
}
