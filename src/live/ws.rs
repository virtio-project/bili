use std::io::Write;
use std::sync::Arc;

use deku::prelude::*;
use flate2::write::ZlibDecoder;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use super::{get_danmaku_info, room_init, DanmakuInfo};
use crate::error::Error;
use crate::live::RoomInit;
use crate::Result;
use std::convert::TryInto;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSplitSink = SplitSink<WsStream, Message>;
type WsSplitStream = SplitStream<WsStream>;

#[derive(Debug, Clone)]
pub struct DanmakuStream {
    inner: Arc<Mutex<DanmakuStreamInner>>,
    fail_over_task: Arc<Mutex<JoinHandle<()>>>,
    pkt_tx: broadcast::Sender<WsPacket>,
}

#[derive(Debug)]
struct DanmakuStreamInner {
    room_info: RoomInit,
    danmaku_info: DanmakuInfo,
    writer: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<()>>,
    srv_index: usize,
    fail_tx: mpsc::Sender<(Instant, Error)>,
    pkt_tx: broadcast::Sender<WsPacket>,
    last_failed: Option<Instant>,
}

impl DanmakuStream {
    pub async fn new(room_id: u64) -> Result<(Self, broadcast::Receiver<WsPacket>)> {
        let room_info = room_init(room_id).await?;
        let danmaku_info = get_danmaku_info(room_info.room_id).await?;
        let (fail_tx, mut fail_rx) = mpsc::channel(1);
        let (pkt_tx, pkt_rx) = broadcast::channel(10);

        let mut inner = DanmakuStreamInner {
            room_info,
            danmaku_info,
            writer: None,
            reader: None,
            srv_index: 0,
            fail_tx,
            pkt_tx: pkt_tx.clone(),
            last_failed: None,
        };

        debug!("init {:?}", inner);

        inner.connect().await?;

        let inner = Arc::new(Mutex::new(inner));

        let _inner = inner.clone();

        let fail_over_task = tokio::spawn(async move {
            while let Some((last_failed, error)) = fail_rx.recv().await {
                error!("error occurred in ws task: {:?}", error);
                let mut inner = _inner.lock().await;
                if let Some(old) = inner.last_failed.replace(last_failed) {
                    let diff = last_failed - old;
                    if diff > Duration::from_millis(100) {
                        if let Err(e) = inner.fail_over().await {
                            error!(
                                "while reset danmaku stream, another error occurred: {:?}",
                                e
                            );
                        } else {
                            info!("danmaku stream has been reset");
                        }
                    }
                }
            }
        });

        Ok((
            Self {
                inner,
                fail_over_task: Arc::new(Mutex::new(fail_over_task)),
                pkt_tx,
            },
            pkt_rx,
        ))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WsPacket> {
        self.pkt_tx.subscribe()
    }
}

impl DanmakuStreamInner {
    fn get_url(&self) -> String {
        let srv = &self.danmaku_info.host_list[self.srv_index];
        format!("wss://{}:{}/sub", srv.host, srv.wss_port)
    }

    async fn fail_over(&mut self) -> Result<()> {
        self.srv_index = (self.srv_index + 1) % self.danmaku_info.host_list.len();
        self.connect().await
    }

    fn terminate(&mut self) {
        if let Some(writer) = self.writer.take() {
            writer.abort();
        }

        if let Some(reader) = self.reader.take() {
            reader.abort();
        }
    }

    async fn connect(&mut self) -> Result<()> {
        let url = self.get_url();
        let (stream, _): (WsStream, _) = tokio_tungstenite::connect_async(&url).await?;
        debug!("ws stream connected to {}", url);

        let (mut ws_writer, ws_reader): (WsSplitSink, WsSplitStream) = stream.split();

        let entering_body =
            EnteringBody::new(self.room_info.room_id, self.danmaku_info.token.clone());
        let pkt = WsPacket::new_json(&entering_body, Operation::Entering)?;
        let payload = pkt.to_bytes()?;
        ws_writer.send(Message::Binary(payload)).await?;
        ws_writer.flush().await?;
        debug!("entering_body sent for {}", self.room_info.room_id);

        self.terminate();
        debug!("reset ws reader/writer task for {}", self.room_info.room_id);

        let fail_tx = self.fail_tx.clone();
        let writer = tokio::spawn(Self::send_heartbeat(ws_writer, fail_tx));
        self.writer = Some(writer);
        debug!(
            "ws writer task (heartbeat) set for {}",
            self.room_info.room_id
        );

        let pkt_tx = self.pkt_tx.clone();
        let fail_tx = self.fail_tx.clone();
        let reader = tokio::spawn(Self::parse_pkt(ws_reader, pkt_tx, fail_tx));
        self.reader = Some(reader);
        debug!("ws reader task set for {}", self.room_info.room_id);

        Ok(())
    }

    async fn parse_pkt(
        mut ws_reader: WsSplitStream,
        pkt_tx: broadcast::Sender<WsPacket>,
        fail_tx: mpsc::Sender<(Instant, Error)>,
    ) {
        async fn parse_pkt_inner(
            ws_reader: &mut WsSplitStream,
            pkt_tx: &broadcast::Sender<WsPacket>,
        ) -> Result<()> {
            if let Some(msg) = ws_reader.next().await {
                let msg = msg?.into_data();
                debug!(
                    "got ws message ({} bytes): {}",
                    msg.len(),
                    hex::encode(&msg)
                );
                let ((rest, _), pkt): ((&[u8], usize), WsPacket) =
                    WsPacket::from_bytes((msg.as_ref(), 0))?;
                if !rest.is_empty() {
                    warn!(
                        "a ws message contains undecoded bytes: {}",
                        hex::encode(&rest)
                    );
                }
                debug!("parse a ws packet: {:?}", pkt);
                if pkt.proto_ver == ProtoVer::ZlibBuf {
                    let mut z = ZlibDecoder::new(Vec::new());
                    z.write_all(pkt.data.as_slice()).map_err(Error::Zlib)?;
                    let buf = z.finish().map_err(Error::Zlib)?;
                    trace!("zlib inner({} bytes): {}", buf.len(), hex::encode(&buf));
                    let mut bytes = buf.as_slice();
                    let mut offset = 0usize;
                    loop {
                        let ((remaining, new_offset), pkt): ((&[u8], usize), WsPacket) =
                            WsPacket::from_bytes((bytes, offset))?;
                        debug!("zlib-ed ws packet found: {:?}", pkt);
                        pkt_tx.send(pkt)?;
                        if remaining.is_empty() {
                            break;
                        }
                        bytes = remaining;
                        offset = new_offset;
                    }
                } else {
                    pkt_tx.send(pkt)?;
                }
            }
            Ok(())
        }

        loop {
            if let Err(e) = parse_pkt_inner(&mut ws_reader, &pkt_tx).await {
                fail_tx.send((Instant::now(), e)).await.unwrap();
                break;
            }
        }
    }

    async fn send_heartbeat(mut ws_writer: WsSplitSink, fail_tx: mpsc::Sender<(Instant, Error)>) {
        async fn send_heartbeat_inner(ws_writer: &mut WsSplitSink) -> Result<()> {
            ws_writer
                .send(Message::Binary(
                    WsPacket::new_heartbeat().to_bytes().unwrap(),
                ))
                .await?;
            ws_writer.flush().await?;
            Ok(())
        }

        loop {
            let checkpoint = Instant::now();
            if let Err(e) = send_heartbeat_inner(&mut ws_writer).await {
                fail_tx.send((Instant::now(), e)).await.unwrap();
            }
            tokio::time::sleep_until(checkpoint + Duration::from_secs(30)).await;
        }
    }
}

#[derive(Clone, Debug, PartialEq, DekuRead, DekuWrite, Serialize, Deserialize)]
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

#[derive(Copy, Clone, Debug, PartialEq, DekuRead, DekuWrite, Serialize, Deserialize)]
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

#[derive(Copy, Clone, Debug, PartialEq, DekuRead, DekuWrite, Serialize, Deserialize)]
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
            data: payload,
        })
    }

    pub fn new_heartbeat() -> Self {
        Self {
            pkt_len: 0,
            hdr_len: 16,
            proto_ver: ProtoVer::Json,
            operation: Operation::HeartBeat,
            seq_id: 1,
            data: vec![],
        }
    }

    /// Get the popularity if this is a heartbeat reply
    pub fn popularity(&self) -> Option<i32> {
        if self.operation == Operation::HeartBeatReply {
            if let Ok(popularity) = self.data.as_slice().try_into() {
                return Some(i32::from_be_bytes(popularity));
            }
            error!(
                "(PLEASE REPORT THIS) unrecognized HeartBeatReply: {}",
                hex::encode(&self.data)
            );
        }
        None
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
            key: "".to_string(),
        }
    }
}

impl EnteringBody {
    pub fn new(room_id: u64, key: String) -> Self {
        Self {
            room_id,
            key,
            ..Default::default()
        }
    }
}
