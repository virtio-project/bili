use deku::prelude::*;
use serde::de::DeserializeOwned;
use serde::{Serialize, Deserialize};

use crate::Result;

#[derive(Debug, PartialEq, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct WsPacket {
    #[deku(bits = "32")]
    #[deku(update = "self.hdr_len + self.data.len()")]
    pub pkt_len: usize,
    #[deku(bits = "16")]
    pub hdr_len: usize,
    pub proto_ver: ProtoVer,
    pub operation: Operation,
    pub seq_id: u32,
    #[deku(count = "pkt_len - hdr_len")]
    pub data: Vec<u8>,
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(type = "u16", endian = "endian", ctx = "endian: deku::ctx::Endian")]
pub enum ProtoVer {
    #[deku(id = "0")]
    Json,
    #[deku(id = "1")]
    Int32BE,
    #[deku(id = "2")]
    ZlibBuf,
    #[deku(id = "3")]
    Unknown,
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(type = "u32", endian = "endian", ctx = "endian: deku::ctx::Endian")]
pub enum Operation {
    #[deku(id = "2")]
    HeartBeat,
    #[deku(id = "3")]
    HeartBeatReply,
    #[deku(id = "5")]
    Notification,
    #[deku(id = "7")]
    Entering,
    #[deku(id = "8")]
    EnteringReply,
}

impl WsPacket {
    pub fn new_json<T: Serialize>(body: &T, operation: Operation) -> Result<Self> {
        let payload = serde_json::to_vec(body)?;
        debug!("{}", String::from_utf8_lossy(payload.as_slice()));
        Ok(Self {
            pkt_len: payload.len() + 16,
            hdr_len: 16,
            proto_ver: ProtoVer::Json,
            operation,
            seq_id: 1,
            data: payload
        })
    }

    pub fn new_heartbeat() -> Self {
        Self {
            pkt_len: 0,
            hdr_len: 16,
            proto_ver: ProtoVer::Json,
            operation: Operation::HeartBeat,
            seq_id: 1,
            data: vec![]
        }
    }

    pub fn decode_body<T: DeserializeOwned>(&self) -> Result<T> {
        if self.proto_ver == ProtoVer::Json {
            Ok(serde_json::from_slice(self.data.as_slice())?)
        } else {
            error!("attempt decode non json body: {:?}", self);
            panic!()
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnteringBody {
    #[serde(default)]
    pub uid: u32,
    #[serde(default)]
    pub platform: String,
    #[serde(default, rename = "protover")]
    pub proto_ver: u8,
    #[serde(rename = "roomid")]
    pub room_id: u64,
    #[serde(default)]
    pub r#type: u8,
    pub key: String,
}


impl Default for EnteringBody {
    fn default() -> Self {
        Self {
            uid: 0,
            platform: "web".to_string(),
            proto_ver: 2,
            room_id: 0,
            r#type: 2,
            key: "".to_string()
        }
    }
}

impl EnteringBody {
    pub fn new(room_id: u64, key: String) -> Self {
        Self {
            room_id,
            key,
            .. Default::default()
        }
    }
}
