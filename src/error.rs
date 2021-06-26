use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("error occurred while make HTTP request: {0:?}")]
    Reqwest(#[from] reqwest::Error),
    #[error("error occurred when serializing/deserializing json: {0:?}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("error occurred in WebSocket: {0:?}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("error occurred while decoding ws packet: {0:?}")]
    WsDecode(#[from] deku::DekuError),
    #[error("error occurred while uncompressing ws packet: {0:?}")]
    Zlib(std::io::Error),
}
