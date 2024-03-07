use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Serialize, Deserialize)]
pub enum Message {
    // Send:
    Login(String),
    
    Ban(IpAddr),
    Unban(IpAddr),
    SetTier(String, u8),
    Logs(u64),
    SaveGrid,
    Restart,
    
    // Receive:
    Unexpected,
    Usage((Vec<(String, f32)>, (u64, u64), Vec<(String, u64, u64)>)),
    LogsResponse(Vec<(String, String, i64)>),
}

pub mod secret;