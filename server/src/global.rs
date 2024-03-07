use crate::{
    blacklist::Blacklist, email::EmailHandler, server::{Tier, ServerHandler},
    urls::Urls,
    payment::Stripe,
};
use std::{
    collections::HashMap,
    sync::Arc,
    net::IpAddr
};
use tokio::sync::{Mutex, RwLock};

pub type AcLimiter = Arc<RwLock<HashMap<IpAddr, bool>>>;

pub struct State {
    pub server: ServerHandler,
    pub blacklist: Blacklist,
    pub smtp: EmailHandler,
    pub urls: Urls,
    pub limiter: AcLimiter,
    pub stripe: Stripe,
}

impl State {
    pub async fn new(server: ServerHandler) -> Self {
        let limiter = Arc::new(RwLock::new(HashMap::new()));

        State {
            server,
            smtp: EmailHandler::init(),
            blacklist: Blacklist,
            urls: Urls::init(limiter.clone()),
            limiter,
            stripe: Stripe::init(),
        }
    }
}
