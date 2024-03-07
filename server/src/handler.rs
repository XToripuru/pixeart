use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    str::FromStr
};

use crate::{
    global,
    server::{PixelUpdate, State, Tier, User, GRID_HEIGHT, GRID_WIDTH},
    DB,
    db::{Record, CheckoutsRecord, LogsRecord}
};
use actix_web::web::Bytes;
use actix_ws::{Message as WsMessage, MessageStream, Session};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{select, sync::mpsc::unbounded_channel, time::interval};
use surrealdb::sql::Id;
use log::debug;

use stripe::{
    Client, CreatePaymentLink, CreatePaymentLinkLineItems, CreatePrice, CreateProduct, Currency,
    IdOrCreate, PaymentLink, Price, Product, CreatePaymentLinkAfterCompletionRedirect, 
    CreatePaymentLinkAfterCompletionType, CreatePaymentLinkAfterCompletion,
    CheckoutSession, CreateCheckoutSession, CreateCheckoutSessionLineItems, CheckoutSessionMode, Timestamp, CheckoutSessionId
};

const HEARTBEAT_TICK: Duration = Duration::from_secs(5);
const TIMEOUT: Duration = Duration::from_secs(30);
const RPS_LIMIT: i32 = 10;
const MAX_REQUEST_SIZE: usize = 1024 * 1024;

#[derive(Serialize)]
pub enum RegistrationError {
    BadEmail,
    BadLogin,
    BadPassword,
    LoginTaken,
    EmailTaken,
}

#[derive(Serialize)]
pub enum TooManyPlace {
    Register,
    Recovery
}

#[derive(Serialize)]
#[repr(u8)]
pub enum Response {
    LoginFailed,
    LoginSuccess(String, String, Tier, i64, bool),
    
    RegistrationFailed(RegistrationError),
    RegistrationSuccess(Tier, i64),

    AlreadyConnected, // 1 ip - 1 device

    Queue(Arc<[PixelUpdate]>),

    PixelUpdateSuccess,
    PixelUpdateFailed,

    AnotherSession,

    BadEmail,
    EmailSent,

    TooMany(TooManyPlace),

    BuyLink(String),
    TierChange(Tier),

    VerifySuccess,
    ResetSuccess,
    ResetFailed,
    LinkExpired,

    Unexpected,
}

#[derive(Deserialize, Debug)]
pub struct Register {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Deserialize, Debug)]
pub struct Login {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Debug)]
pub enum Message {
    Login(Login),
    Register(Register),
    Logout,
    SetPixel {
        color: [u8; 3],
        idx: usize,
    },
    Recovery(String),
    Buy(u8),
    #[serde(skip)]
    Queue(Arc<[PixelUpdate]>),
    #[serde(skip)]
    AnotherSession,
    #[serde(skip)]
    Verified,
    #[serde(skip)]
    TierChange(Tier)
}

pub async fn ws_handler(
    mut stream: MessageStream,
    mut session: Session,
    global: Arc<global::State>,
    state: State,
    ip: SocketAddr,
) {
    if !global.blacklist.check(ip.ip()).await {
        let _ = session.close(None).await;
        return;
    }

    let (tx, mut rx) = unbounded_channel::<Message>();
    let conn_id = match global.server.connect(tx, state, ip).await {
        Some((id, grid)) => {
            let _ = session.binary(grid).await;

            id
        }
        None => {
            let response = serde_json::to_string(&Response::AlreadyConnected).unwrap();
            let _ = session.text(response).await;
            let _ = session.close(None).await;

            return;
        }
    };

    let mut user = None;

    let mut hb = Instant::now();
    let mut hb_interval = interval(HEARTBEAT_TICK);

    let mut rps = 0; // requests per second
    let mut rq_interval = interval(std::time::Duration::from_secs(1));

    let reason = loop {
        select! {
            _ = rq_interval.tick() => {
                if rps > RPS_LIMIT {
                    global.blacklist.timeout(ip.ip()).await;
                    break None;
                }

                rps = 0;
            }
            _ = hb_interval.tick() => {
                if Instant::now().duration_since(hb) > TIMEOUT || session.ping(b"").await.is_err() {
                    break None;
                }
            }
            Some(Ok(msg)) = stream.recv() => {
                rps += 1;

                match msg {
                    WsMessage::Text(text) => {
                        if payload_overflow(text.as_bytes()) {
                            global.blacklist.timeout(ip.ip()).await;
                            break None;
                        }

                        let Ok(msg) = serde_json::from_str::<Message>(&text) else { continue };

                        match process_msg(msg, global.clone(), conn_id, &mut user, ip.ip()).await {
                            Some(response) => {
                                let response = serde_json::to_string(&response).unwrap();

                                let _ = session.text(response).await;
                            },
                            None => {}
                        }
                    }
                    WsMessage::Ping(bytes) => {
                        if payload_overflow(&bytes) {
                            global.blacklist.timeout(ip.ip()).await;
                            break None;
                        }

                        hb = Instant::now();

                        let _ = session.pong(&bytes).await;
                    }
                    WsMessage::Pong(bytes) => {
                        if payload_overflow(&bytes) {
                            global.blacklist.timeout(ip.ip()).await;
                            break None;
                        }

                        hb = Instant::now();
                    }
                    WsMessage::Close(reason) => {
                        break reason;
                    }
                    WsMessage::Binary(bytes) => {
                        if payload_overflow(&bytes) {
                            global.blacklist.timeout(ip.ip()).await;
                            break None;
                        }
                    }
                    _ => {}
                }
            }
            Some(msg) = rx.recv() => {
                match msg {
                    Message::Queue(queue) => {
                        let response = serde_json::to_string(&Response::Queue(queue)).unwrap();

                        let _ = session.text(response).await;
                    },
                    Message::AnotherSession => {
                        user = None;
                        let response = serde_json::to_string(&Response::AnotherSession).unwrap();

                        let _ = session.text(response).await;
                    }
                    Message::Verified => {
                        let Some(ref mut user) = user else { continue };
                       
                        let _: Option<Record> = match DB.create("logs")
                            .content(LogsRecord::new(ip.ip().to_string(), format!("User {} verified", user.email)))
                            .await 
                        {
                            Ok(x) => x,
                            Err(_) => None
                        };
                        user.verified = true;

                        let response = serde_json::to_string(&Response::VerifySuccess).unwrap();

                        let _ = session.text(response).await;
                    }
                    Message::TierChange(tier) => {
                        let Some(ref mut user) = user else { continue; };
                        user.tier = tier;

                        let response = serde_json::to_string(&Response::TierChange(tier)).unwrap();

                        let _ = session.text(response).await;
                    }
                    _ => unreachable!()
                }
            }
        }
    };

    global.server.disconnect(conn_id);
    let _ = session.close(reason).await;
}

fn is_ascii_alphanumeric(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn payload_overflow(bytes: &Bytes) -> bool {
    bytes.len() > MAX_REQUEST_SIZE
}

// username - azAZ0-9, spaces allowed (limited), 32
fn check_nick(nick: &str) -> bool {
    nick.split(" ")
        .all(|s| is_ascii_alphanumeric(s) && s.len() > 0)
        && nick.len() <= 32
}

// password - azAZ0-9, no spaces, 32
pub fn check_password(password: &str) -> bool {
    is_ascii_alphanumeric(password) && password.len() <= 32 && password.len() >= 8
}

fn check_email(email: &str) -> bool {
    let mut split = email.split("@");
    let Some(before) = split.next() else { return false };
    let Some(after) = split.next() else { return false };

    let mut split = after.split(".");
    let Some(domain) = split.next() else { return false };
    let Some(tld) = split.next() else { return false };

    email.len() <= 64 && before.len() > 0 && domain.len() > 0 && tld.len() > 0
}

fn valid_pixel(idx: usize) -> bool {
    idx < (GRID_WIDTH * GRID_HEIGHT) as usize
}

async fn process_msg(
    msg: Message,
    global: Arc<global::State>,
    conn_id: i64,
    user_copy: &mut Option<User>,
    ip: IpAddr
) -> Option<Response> {
    match msg {
        Message::Register(Register {
            username,
            email,
            password,
        }) => {
            debug!("Registering");

            if !check_nick(&username) {
                return Some(Response::RegistrationFailed(RegistrationError::BadLogin));
            }
            if !check_password(&password) {
                return Some(Response::RegistrationFailed(RegistrationError::BadPassword));
            }
            if !check_email(&email) {
                return Some(Response::RegistrationFailed(RegistrationError::BadEmail));
            }

            match global.limiter.read().await.get(&ip) {
                Some(true) | None => {},
                Some(false) => return Some(Response::TooMany(TooManyPlace::Register))
            }

            let res: Option<User> = match DB.query("SELECT * FROM type::table($table) WHERE email=$email OR name=$name")
            .bind(("table", "users"))
            .bind(("email", &*email))
            .bind(("name", &*username))
            .await
            {
                Ok(mut res) => {
                    match res.take(0) {
                        Ok(x) => x,
                        Err(_) => None 
                    }
                },
                Err(_) => None
            };

            if let Some(user) = res {
                if user.name == username {
                    return Some(Response::RegistrationFailed(RegistrationError::LoginTaken));
                } 
                if user.email == email {
                    return Some(Response::RegistrationFailed(RegistrationError::EmailTaken));
                }
            }

            let mut hasher = Sha256::new();

            hasher.update(password.as_bytes());
            let result = hasher.finalize();

            let result: [u8; 32] = result[..].try_into().unwrap();

            let user = User::new(username, email.clone(), result);
            
            let _: User = match DB.create(("users", user.email.clone())).content(user.clone()).await
            {
                Ok(x) => x,
                Err(_) => {
                    // logging required
                    return Some(Response::Unexpected);
                }
            };

            global.server.upgrade(conn_id, user.clone()).await;

            *user_copy= Some(user.clone());

            let url = global.urls.generate(crate::urls::Url::Verify, &email, ip).await;
            let _ = global.smtp.verify(&user.email, &url).await;

            let _: Option<Record> = match DB.create("logs")
                .content(LogsRecord::new(ip.to_string(), format!("User {} registered", email)))
                .await 
            {
                Ok(x) => x,
                Err(_) => None
            };

            Some(Response::RegistrationSuccess(user.tier, user.last))
        }
        Message::Login(Login { username, password }) => {
            if user_copy.is_some() {
                return None;
            }

            if let Some(mut user) = match DB.query("SELECT * FROM type::table($table) WHERE email=$email OR name=$name")
            .bind(("table", "users"))
            .bind(("email", &*username))
            .bind(("name", &*username))
            .await {
                Ok(mut res) => {
                    match res.take(0) {
                        Ok(x) => x,
                        Err(err) => {
                            None::<User>
                        }
                    }
                }
                Err(err) => {
                    None
                }
            } {
                let mut hasher = Sha256::new();

                hasher.update(password.as_bytes());
                let result = hasher.finalize();

                let result: [u8; 32] = result[..].try_into().unwrap();

                if (user.email == username || user.name == username) && user.password == result {
                    if let Some((tier, last)) = global.server.upgrade(conn_id, user.clone()).await {
                        user.tier = tier;
                        user.last = last;
                    }

                    *user_copy= Some(user.clone());

                    let _: Option<Record> = match DB.create("logs")
                        .content(LogsRecord::new(ip.to_string(), format!("User {} logged", user.email)))
                        .await 
                    {
                        Ok(x) => x,
                        Err(_) => None
                    };

                    return Some(Response::LoginSuccess(
                        user.name,
                        user.email,
                        user.tier,
                        user.last,
                        user.verified,
                    ));
                }
            }

            Some(Response::LoginFailed)
        }
        Message::Logout => {
            if user_copy.is_none() {
                return None;
            }

            global.server.downgrade(conn_id);
            *user_copy  = None;

            None
        }
        Message::SetPixel { color, idx } => {
            let Some(user) = user_copy else { return None; };
            if user.verified && valid_pixel(idx) {
                return match global.server.update_pixel(conn_id, color, idx).await {
                    Ok(_) => Some(Response::PixelUpdateSuccess),
                    Err(err) => Some(Response::PixelUpdateFailed),
                };
            }

            None
        }
        Message::Recovery(email) => {
            let res: Option<User> = match DB.select(("users", &*email)).await {
                Ok(x) => x,
                Err(_) => return Some(Response::Unexpected)
            };

            if res.is_none() || !check_email(&email) {
                return Some(Response::BadEmail);
            }            

            match global.limiter.read().await.get(&ip) {
                Some(true) | None => {},
                Some(false) => return Some(Response::TooMany(TooManyPlace::Register))
            }

            let url = global.urls.generate(crate::urls::Url::Reset, &email, ip).await;
            let _ = global.smtp.pswd_reset(&email, &url).await;

            let _: Option<Record> = match DB.create("logs")
                .content(LogsRecord::new(ip.to_string(), format!("User {} recovering password", email)))
                .await 
            {
                Ok(x) => x,
                Err(_) => None
            };

            Some(Response::EmailSent)
        }
        Message::Buy(tier) => {
        
            let Some(user) = user_copy else { return Some(Response::Unexpected) };
            let Some(tier) = Tier::from_numeric(tier) else { return Some(Response::Unexpected); };
            
            let record: Option<Record> = match DB.query("SELECT id FROM type::table($table) WHERE email=$email")
            .bind(("table", "checkouts"))
            .bind(("email", &*user.email))
            .await {
                Ok(mut res) => {
                    match res.take(0) {
                        Ok(res) => res,
                        Err(err) => {
                            None
                        }
                    }
                }
                Err(err) => {
                    None
                }
            };

            match record {
                Some(record) => {
                    let id = match record.id.id {
                        Id::String(id) => id,
                        _ => return Some(Response::Unexpected)
                    };

                    let Ok(cid) = CheckoutSessionId::from_str(&id) else { return Some(Response::Unexpected) };
                    if let Err(_) = CheckoutSession::expire(&global.stripe.client, &cid).await {
                        return Some(Response::Unexpected);
                    }

                    let _: Option<CheckoutsRecord> =  match DB.delete(("checkouts", &*id)).await {
                        Ok(x) => x,
                        Err(_) => {
                            //logs
                            None
                        }
                    };
                }
                None => {}
            }

            //let records: Vec<CheckoutsRecord> = DB.select("checkouts").await.unwrap();

            if tier <= user.tier {
                return Some(Response::Unexpected);
            }

            let _: Option<Record> = match DB.create("logs")
                .content(LogsRecord::new(ip.to_string(), format!("User {} buying tier {}", user.email, tier)))
                .await 
            {
                Ok(x) => x,
                Err(_) => None
            };

            // let price = tier.price() - user_tier.price();

            // let name = format!("{tier}");

            // let Ok(product) = ({
            //     let mut create_product = CreateProduct::new(&name);
            //     Product::create(&global.client, create_product).await
            // }) else {
            //     println!("Error during creating product");
            //     return Some(Response::Unexpected) 
            // };

            // let Ok(price) = ({
            //     let mut create_price = CreatePrice::new(Currency::USD);
            //     create_price.product = Some(IdOrCreate::Id("prod_ONSl7KvtXWhSMU"));
            //     create_price.unit_amount = Some(price as i64 * 100);
            //     create_price.expand = &["product"];
            //     Price::create(&global.client, create_price).await
            // }) else { 
            //     println!("Error during creating price");
            //     return Some(Response::Unexpected) 
            // };

            let mut params = CreateCheckoutSession::new("https://pixeart.online/thanks");

            params.line_items = Some(vec![
                CreateCheckoutSessionLineItems {
                    price: Some(String::from(global.stripe.get_id(user.tier, tier))),
                    quantity: Some(1),
                    ..Default::default()
                }
            ]);

            params.currency = Some(Currency::USD);
            params.expires_at = Some(
                (SystemTime::now() + Duration::from_secs(35 * 60))
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64
            );
            params.mode = Some(CheckoutSessionMode::Payment);

            let session = match CheckoutSession::create(&global.stripe.client, params).await 
            { 
                Ok(session) => session,
                Err(err) => {
                    println!("{err}:#?");
                    return Some(Response::Unexpected); 
                }
            };
                
            let id = session.id;
            let url = session.url.unwrap();

            let _: CheckoutsRecord = match DB.create(("checkouts", id.to_string())).content(CheckoutsRecord {
                ip: ip.to_string(),
                email: user.email.clone(),
                tier
            }).await {
                Ok(x) => x,
                Err(err) => {
                    println!("{err:#?}");
                    return Some(Response::Unexpected);
                }
            };

            debug!("id: {id}\n, url: {url}");

            Some(Response::BuyLink(url))
        }
        Message::Queue(_) | Message::AnotherSession | Message::Verified | Message::TierChange(_) => None,
    }
}
