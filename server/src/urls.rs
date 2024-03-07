use chrono::Utc;
use rand::seq::SliceRandom;
use std::{collections::HashMap, sync::Arc, net::IpAddr};
use tokio::{
    select,
    spawn,
    sync::{mpsc, Mutex}
};
use futures::{
    StreamExt,
    stream::FuturesUnordered
};
use crate::{DB, global::AcLimiter, server::User};

const URL_LIFE: i64 = 5 * 60;
const ASCII: [char; 36] = [
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S',
    'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
];

#[derive(Clone)]
pub enum Url {
    Verify,
    Reset,
}

impl Url {
    fn generate(&self) -> String {
        let rng = &mut rand::thread_rng();

        let mut url = String::from("https://pixeart.online/");
        let id = ASCII.choose_multiple(rng, 20).collect::<String>();

        match self {
            Url::Verify => {
                url.push_str("verify/");
                url.push_str(&id);
            }
            Url::Reset => {
                url.push_str("recovery/");
                url.push_str(&id);
            }
        }

        url
    }
    pub fn from_id(&self, id: &str) -> String {
        let mut url = String::from("https://pixeart.online/");

        match self {
            Url::Verify => {
                url.push_str("verify/");
                url.push_str(&id);
            }
            Url::Reset => {
                url.push_str("recovery/");
                url.push_str(&id);
            }
        }

        url
    }
}

type UrlList = Arc<Mutex<HashMap<String, (i64, String)>>>;

#[derive(Clone)]
pub struct Urls {
    v_urls: UrlList, // verification urls
    p_urls: UrlList, // password reset urls

    tx: mpsc::Sender<(Url, String, IpAddr)>,
}

impl Urls {
    pub fn init(limiter: AcLimiter) -> Self {
        let v_urls = Arc::new(Mutex::new(HashMap::<String, (i64, String)>::new()));
        let p_urls = Arc::new(Mutex::new(HashMap::<String, (i64, String)>::new()));
        let (tx, mut rx) = mpsc::channel::<(Url, String, IpAddr)>(32);
        
        spawn({
            let v_urls = v_urls.clone();
            let p_urls = p_urls.clone();

            async move {
                let mut queue =  FuturesUnordered::new();
                loop {
                    select! {
                        Some(res) = rx.recv() => {
                            limiter.write().await.insert(res.2, false);
                            queue.push(async move { 
                                tokio::time::sleep(std::time::Duration::from_secs(URL_LIFE as u64)).await; 
                                
                                res
                            });
                        }
                        Some((ty, url, ip)) = queue.next() => {
                            let mut lock = match ty {
                                Url::Verify => v_urls.lock().await,
                                Url::Reset => p_urls.lock().await
                            };

                            if let Some((_, email)) = lock.remove(&url) {
                                limiter.write().await.insert(ip, true);

                                let _: Option<User> = DB.delete(("users", &*email)).await.unwrap_or(None);
                            }
                        }
                    }
                }
            }
        });

        Urls {
            v_urls,
            p_urls,
            tx
        }
    }
    pub async fn generate(&self, ty: Url, email: &str, ip: IpAddr) -> String {
        let url = ty.generate();
        let now = Utc::now().timestamp();

        match ty.clone() {
            Url::Verify => {
                self.v_urls
                    .lock()
                    .await
                    .insert(url.clone(), (now, String::from(email)));
            }
            Url::Reset => {
                self.p_urls
                    .lock()
                    .await
                    .insert(url.clone(), (now, String::from(email)));
            }
        }

        self.tx.send((ty, url.clone(), ip)).await.unwrap();

        url
    }
    // Some(x) - exist, x false if expired
    pub async fn exist(&self, ty: Url, url: &str) -> Option<bool> {
        let now = Utc::now().timestamp();

        let lock = match ty {
            Url::Verify => self.v_urls.lock().await,
            Url::Reset => self.p_urls.lock().await
        };

        let Some((t, _)) = lock.get(url) else { return None };
        //let Some((t, _)) = res else { return None; };

        if now - t < URL_LIFE {
            return Some(true);
        } else {
            None
        }
    }
    pub async fn remove(&self, ty: Url, url: &str) -> Option<String> {
        let now = Utc::now().timestamp();

        let res = match ty {
            Url::Verify => self.v_urls.lock().await.remove(url),
            Url::Reset => self.p_urls.lock().await.remove(url),
        };

        let Some((t, email)) =  res else { return None };

        if now - t < URL_LIFE {
            return Some(email);
        } else {
            None
        }
    }
}
