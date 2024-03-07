use chrono::Utc;
use std::{collections::HashMap, net::IpAddr, sync::Arc};
use tokio::{fs, spawn, sync::{Mutex, RwLock}};
use crate::{DB, db::BlacklistRecord};

const TIMEOUT: i64 = 5 * 60;

#[derive(Clone)]
pub struct Blacklist;

impl Blacklist {
    pub async fn init() -> Self {
        Blacklist
    }
    pub async fn check(&self, ip: IpAddr) -> bool {
        let res: Option<BlacklistRecord> = match DB.select(("blacklist", ip.to_string())).await {
            Ok(x) => x,
            Err(_) => return true
        };

        let Some(BlacklistRecord { last, timeouts }) = res else { return true; };
        let now = Utc::now().timestamp();

        now - last > TIMEOUT && timeouts < 3
    }
    pub async fn timeout(&self, ip: IpAddr) {
        //self.logs.lock().await.log(format!("{ip} gets a timeout")).await;
        let now = Utc::now().timestamp();

        let _: Option<BlacklistRecord> = match DB
            .query("UPDATE type::thing(type::table($table), type::string($id)) SET last = $now, timeouts += 1")
            .bind(("table", "blacklist"))
            .bind(("id", ip.to_string()))
            .bind(("now", now))
            .await 
        {
            Ok(mut res) => {
                match res.take(0) {
                    Ok(x) => x,
                    Err(_) => {
                        None
                    }
                }
            },
            Err(_) => None
        };
    }
    pub async fn ban(&self, ip: IpAddr) {
        //self.logs.lock().await.log(format!("{ip} gets a ban")).await;

        let now = Utc::now().timestamp();

        let _: Option<BlacklistRecord> = match DB
            .query("UPDATE type::thing(type::table($table), type::string($id)) SET last = $now, timeouts = 3")
            .bind(("table", "blacklist"))
            .bind(("id", ip.to_string()))
            .bind(("now", now))
            .await
        {
            Ok(mut res) => {
                match res.take(0) {
                    Ok(x) => x,
                    Err(_) => {
                        None
                    }
                }
            },
            Err(_) => None
        };
    }
    pub async fn unban(&self, ip: IpAddr) {
        //self.logs.lock().await.log(format!("{ip} gets an unban")).await;

        let _: Option<BlacklistRecord> = match DB.delete(("blacklist", ip.to_string())).await {
            Ok(x) => x,
            Err(_) => {
                None
            }
        };
    }
}
