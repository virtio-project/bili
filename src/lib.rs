//! `bili` is a library for interacting
//! with [bilibili](https://bilibili.com).
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/RedCircleProject/bili/master/bili.png"
)]
#[macro_use]
extern crate log;

use serde::{Deserialize, Serialize};

mod error;
pub mod live;
pub use error::{Error, Result};

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Bilibili API response wrapper
///
/// Have no idea for the difference between those two `msg` fields.
pub struct ApiResponse<T> {
    code: i64,
    msg: Option<String>,
    message: Option<String>,
    data: Option<T>,
}

impl<T> ApiResponse<T> {
    /// Assume that only code `0` stands for ok.
    pub fn ok(&self) -> bool {
        self.code == 0
    }

    /// Get the status code, note this is a signed integer.
    pub fn code(&self) -> i64 {
        self.code
    }

    /// Get the msg.
    pub fn msg(&self) -> Option<&str> {
        self.msg.as_deref()
    }

    /// Get the message.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Get the data ref.
    pub fn data(&self) -> Option<&T> {
        self.data.as_ref()
    }

    /// Unwrap into the data.
    pub fn into_data(self) -> T {
        self.data.unwrap()
    }
}
