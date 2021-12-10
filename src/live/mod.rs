use crate::{ApiResponse, Result};
use serde::{Deserialize, Serialize};

pub mod consts;
pub mod ws;

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
/// Living room Info.
pub struct RoomInit {
    pub room_id: u64,
    pub short_id: u64,
    pub uid: u64,
    pub need_p2p: u64,
    pub is_hidden: bool,
    pub is_locked: bool,
    pub is_portrait: bool,
    pub live_status: u64,
    pub hidden_till: u64,
    pub lock_till: u64,
    pub encrypted: bool,
    pub pwd_verified: bool,
    pub live_time: i64,
    pub room_shield: u64,
    pub is_sp: u64,
    pub special_type: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// DanmakuInfo
pub struct DanmakuInfo {
    pub group: String,
    pub business_id: u32,
    pub refresh_row_factor: f64,
    pub refresh_rate: u32,
    pub max_delay: u32,
    pub token: String,
    pub host_list: Vec<DanmakuHost>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// DanmakuHost information
pub struct DanmakuHost {
    pub host: String,
    pub port: u16,
    pub wss_port: u16,
    pub ws_port: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Playback Url Infos
pub struct PlayUrlInfos {
    pub current_quality: u16,
    pub accept_quality: Vec<String>,
    pub current_qn: u32,
    pub quality_description: Vec<QualityDescription>,
    pub durl: Vec<PlayUrl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Quality description
pub struct QualityDescription {
    pub qn: u32,
    pub desc: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Playback Url
pub struct PlayUrl {
    pub url: String,
    pub length: u16,
    pub order: u16,
    pub stream_type: u16,
    pub p2p_type: u16,
}

/// Get the living room info.
pub async fn room_init(room_id: u64) -> Result<RoomInit> {
    let url = format!("{}?id={}", consts::ROOM_INIT, room_id);
    debug!("room_init request to: {}", url);
    let response: ApiResponse<RoomInit> = reqwest::get(url).await?.json().await?;
    debug!("response: {}", serde_json::to_string(&response).unwrap());
    Ok(response.into_data())
}

/// Get the danmaku server info.
pub async fn get_danmaku_info(room_id: u64) -> Result<DanmakuInfo> {
    let url = format!("{}?id={}&type=0", consts::DANMAKU_SERVER_CONF, room_id);
    debug!("get_danmaku_info request to: {}", url);
    let response: ApiResponse<DanmakuInfo> = reqwest::get(url).await?.json().await?;
    debug!("response: {}", serde_json::to_string(&response).unwrap());
    Ok(response.into_data())
}

pub async fn get_play_url_info(room_id: u64) -> Result<PlayUrlInfos> {
    let url = format!("{}?cid={}&platform=web", consts::PLAY_URL, room_id);
    debug!("get_play_url_info request to: {}", url);
    let response: ApiResponse<PlayUrlInfos> = reqwest::get(url).await?.json().await?;
    debug!("response: {}", serde_json::to_string(&response).unwrap());
    Ok(response.into_data())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_room_init() {
        pretty_env_logger::try_init().ok();
        let resp = room_init(14507014).await.unwrap();
        info!("{:?}", resp);
        assert_eq!(resp.room_id, 14507014);
    }

    #[tokio::test]
    async fn test_get_danmaku_info() {
        pretty_env_logger::try_init().ok();
        let resp = get_danmaku_info(14507014).await.unwrap();
        info!("{:?}", resp);
        assert!(resp.host_list.len() > 0);
    }

    #[tokio::test]
    async fn test_get_play_url_info() {
        pretty_env_logger::try_init().ok();
        let resp = get_play_url_info(14507014).await.unwrap();
        info!("{:?}", resp);
        assert!(resp.durl.len() > 0);
    }
}
