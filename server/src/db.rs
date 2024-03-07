use crate::server::Tier;
use surrealdb::{
    Surreal,
    engine::remote::ws::{Ws, Client},
    sql::Thing,
    opt::auth::Root,
    opt::PatchOp,
};
use serde::{Serialize, Deserialize};
use chrono::Utc;
use std::net::IpAddr;

#[derive(Serialize, Deserialize, Debug)]
pub struct Record {
    pub id: Thing,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BlacklistRecord {
    pub last: i64,
    pub timeouts: u8
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CheckoutsRecord {
    pub ip: String,
    pub email: String,
    pub tier: Tier,
}

#[derive(Serialize, Debug)]
pub struct TierUpdate {
    pub tier: Tier
}

#[derive(Serialize, Debug)]
pub struct Pixel {
    pub rgb: [u8; 3]
}

#[derive(Deserialize, Serialize, Debug)]
pub struct LogsRecord {
    pub ip: String,
    pub msg: String, 
    pub timestamp: i64
}

impl LogsRecord {
    pub fn new(ip: String, msg: String) -> Self {
        LogsRecord { ip, msg, timestamp: Utc::now().timestamp() }
    }
}