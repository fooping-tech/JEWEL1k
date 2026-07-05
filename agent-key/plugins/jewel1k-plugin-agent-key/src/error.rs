use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Transport(#[from] agent_key_core::TransportError),
    #[error("no device connected")]
    NotConnected,
    #[error("unknown device id: {0}")]
    UnknownDevice(String),
    #[error("approval not found: {0}")]
    ApprovalNotFound(String),
    #[error("{0}")]
    InvalidInput(String),
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
