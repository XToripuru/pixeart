mod db;
mod blacklist;
mod email;
mod global;
mod handler;
mod panel;
mod server;
mod urls;
mod payment;

use blacklist::Blacklist;
use handler::{check_password, ws_handler, Response};
use sha2::{Digest, Sha256};
use crate::{db::{TierUpdate, CheckoutsRecord, LogsRecord, Record}, server::{Tier, User}};
use actix_files::Files;
use actix_web::{get, post, web, App, HttpRequest, HttpResponse, HttpServer, http::StatusCode, body::BoxBody};
use server::{Server, State};
use tokio::{spawn, task::spawn_local};
use futures_util::stream::StreamExt;
use stripe::{EventType, Webhook, EventObject};
use serde_json::json;
use surrealdb::{
    Surreal,
    engine::remote::ws::{Ws, Client},
    sql::Thing,
    opt::auth::Root,
    opt::PatchOp,
};
use rustls::{Certificate, PrivateKey, ServerConfig};
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::{
    fs::File,
    io::BufReader,
    error::Error
};
use actix_extensible_rate_limit::{
    backend::{
        SimpleInputFunctionBuilder,
        memory::InMemoryBackend,
    },
    RateLimiter
};
use log::debug;

const VERIFY_FILE: &str = include_str!("../static/index/verify.html");
const RECOVERY_FILE: &str = include_str!("../static/index/recovery.html");
const INDEX_FILE: &str = include_str!("../static/index/main.html");
const THANKS_FILE: &str = include_str!("../static/index/thanks.html");
const CONTACT_FILE: &str = include_str!("../static/contact.html");
const PP_FILE: &str = include_str!("../static/pp.html");
const TOS_FILE: &str = include_str!("../static/tos.html");

pub static DB: Surreal<Client> = Surreal::init();

async fn pixels(
    req: HttpRequest,
    data: web::Data<global::State>,
    stream: web::Payload,
) -> Result<HttpResponse, Box<dyn Error>> {
    let (res, session, msg_stream) = actix_ws::handle(&req, stream)?;

    // TODO: No logging out
    let info = State::Unverified;
    spawn_local(ws_handler(
        msg_stream,
        session,
        (*data).clone(),
        info,
        req.peer_addr().unwrap(),
    ));

    Ok(res)
}

#[get("/verify/{id}")]
async fn verify_page(req: HttpRequest, path: web::Path<String>, global: web::Data<global::State>) -> Result<HttpResponse, Box<dyn Error>> {
    debug!("Verificating");

    let id = path.into_inner();
    let ty = urls::Url::Verify;
    let ip = req.peer_addr().unwrap().ip();

    match global.urls.remove(ty.clone(), &ty.from_id(&id)).await {
        Some(email) => {
            let res: Option<User> = DB
                .update(("users", &*email))
                .merge(json!({
                    "verified": true
                }))
                .await?;

            if res.is_some() {
                global.server.verify(email).await;
                global.limiter.write().await.insert(ip, true);

                Ok(HttpResponse::with_body(StatusCode::OK, BoxBody::new(VERIFY_FILE)))
            } else {
                Ok(HttpResponse::NotFound().finish())
            }
        }
        None => Ok(HttpResponse::NotFound().finish()),
    }
}

#[get("/recovery/{id}")]
async fn recovery_page(path: web::Path<String>, data: web::Data<global::State>) -> HttpResponse {
    let id = path.into_inner();
    let ty = urls::Url::Reset;

    match data.urls.exist(ty.clone(), &ty.from_id(&id)).await {
        Some(true) => HttpResponse::with_body(StatusCode::OK, BoxBody::new(RECOVERY_FILE)),
        None | Some(false) => HttpResponse::NotFound().finish()
    }
}

#[get("/thanks")]
async fn thanks_page() -> HttpResponse {
    HttpResponse::with_body(StatusCode::OK, BoxBody::new(THANKS_FILE))
}

#[get("/contact")]
async fn contact_page() -> HttpResponse {
    HttpResponse::with_body(StatusCode::OK, BoxBody::new(CONTACT_FILE))
}

#[get("/tos")]
async fn tos_page() -> HttpResponse {
    HttpResponse::with_body(StatusCode::OK, BoxBody::new(TOS_FILE))
}

#[get("/pp")]
async fn pp_page() -> HttpResponse {
    HttpResponse::with_body(StatusCode::OK, BoxBody::new(PP_FILE))
}

#[post("/recovery/{id}")]
async fn reset(
    req: HttpRequest,
    path: web::Path<String>,
    global: web::Data<global::State>,
    info: web::Json<String>,
) -> Result<web::Json<Response>, Box<dyn Error>> {
    let id = path.into_inner();
    let ty = urls::Url::Reset;
    let password = info.0;

    match global.urls.remove(ty.clone(), &ty.from_id(&id)).await {
        Some(email) => {
            if !check_password(&password) {
                return Ok(web::Json(Response::ResetFailed));
            }
            
            global.limiter
                .write()
                .await
                .remove(&req.peer_addr().unwrap().ip());

            let mut hasher = Sha256::new();

            hasher.update(password.as_bytes());
            let result = hasher.finalize();

            let result: [u8; 32] = result[..].try_into().unwrap();

            let res: Option<User> = DB.update(("users", &*email))
                .merge(json!({
                    "password": result
                }))
                .await?;

            if res.is_some() {
                Ok(web::Json(Response::ResetSuccess))
            } else {
                Ok(web::Json(Response::Unexpected))
            }
        }
        None => Ok(web::Json(Response::LinkExpired)),
    }
}

#[post("/webhook")]
async fn webhook(req: HttpRequest, mut body: web::Payload, global: web::Data<global::State>) -> Result<HttpResponse, Box<dyn Error>> {
    //println!("{:?}", body);
    debug!("Got request on webhook");
    let Some(header) = req.headers().get("stripe-signature") else { return Ok(HttpResponse::Ok().finish()); };
    
    let mut bytes = web::BytesMut::new();
    while let Some(item) = body.next().await {
        let item = item?;
        bytes.extend_from_slice(&item);
    }

    let event = Webhook::construct_event(
        std::str::from_utf8(&bytes[..])?,
        header.to_str()?,
        shared::secret::STRIPE_WEBHOOK_SECRET_KEY
    )?;

    let data = event.data;
    let ty = event.type_;

    //println!("data: {:#?}\nty: {:#?}", data, ty);

    match ty {
        EventType::CheckoutSessionCompleted => {
            let obj = data.object;

            match obj {
                EventObject::CheckoutSession(checkout) => {
                    let id = checkout.id.to_string();
                    
                    //let x: Vec<CheckoutsRecord> = DB.select("checkouts").await.unwrap();
                    //println!("{x:#?}");

                    let Some(CheckoutsRecord { ip, email, tier }) = DB.delete(("checkouts", id)).await? else { return Ok(HttpResponse::Ok().finish()); };

                    // println!("{tier} {email}");
                    // let _: Option<User> = DB.update(("users", &*email))
                    //     .merge(TierUpdate {tier})
                    //     .await.unwrap();

                    let _: Option<User> = DB.query("UPDATE type::thing(type::table($table), type::string($user)) SET tier=$tier")
                        .bind(("table", "users"))
                        .bind(("user", &*email))
                        .bind(("tier", tier))
                        .await?
                        .take(0)?;
                    //DB.query("UPDATE x SET tier=$tier")
                    //.bind(("tier", ))

                    let _: Option<Record> = match DB.create("logs")
                        .content(LogsRecord::new(ip, format!("User {} bought tier {}", email, tier)))
                        .await 
                    {
                        Ok(x) => x,
                        Err(_) => None
                    };

                    //let user: Vec<User> = DB.select("users").await.unwrap();
                    //println!("{user:#?}");

                    global.server.tier_change(email, tier).await;
                },
                _ => {}
            }

        }
        _ => {}
    }

    Ok(HttpResponse::Ok().finish())
}

#[get("/")]
async fn index() -> HttpResponse {
    HttpResponse::with_body(StatusCode::OK, BoxBody::new(INDEX_FILE))
}

fn load_rustls_config() -> rustls::ServerConfig {
    // init server config builder with safe defaults
    let config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth();

    // load TLS key/cert files
    let cert_file = &mut BufReader::new(File::open("./server/pixeart.online.chained.pem").unwrap());
    let key_file = &mut BufReader::new(File::open("./server/pixeart.online.key.pem").unwrap());

    // convert files to key/cert objects
    let cert_chain = certs(cert_file)
        .unwrap()
        .into_iter()
        .map(Certificate)
        .collect();
    let mut keys: Vec<PrivateKey> = pkcs8_private_keys(key_file)
        .unwrap()
        .into_iter()
        .map(PrivateKey)
        .collect();

    // exit if no keys could be parsed
    if keys.is_empty() {
        eprintln!("Could not locate PKCS 8 private keys.");
        std::process::exit(1);
    }

    config.with_single_cert(cert_chain, keys.remove(0)).unwrap()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    DB.connect::<Ws>("127.0.0.1:8000").await.unwrap();

    if let Err(_) = DB.query("USE NS toripuru").await {
        DB.query("DEFINE NAMESPACE toripuru").await.unwrap();
        // DB.query("USE NS toripuru").await.unwrap();
    }

    if let Err(_) = DB.query("USE DB pixeart").await {
        DB.query("DEFINE DATABASE pixeart").await.unwrap();
        // DB.query("USE DB pixeart").await.unwrap();
    }
    
    DB.signin(Root {
        username: "root",
        password: "root"
    }).await.unwrap();

    DB.use_ns("toripuru").use_db("pixeart").await.unwrap();


    let (server, handler) = Server::new().await;

    let panel = panel::Panel::new(Blacklist, handler.clone());

    let global = web::Data::new(global::State::new(handler).await);

    let backend = InMemoryBackend::builder().build();

    spawn(server.run());
    spawn(panel.run());

    HttpServer::new(move || {
        let input = SimpleInputFunctionBuilder::new(std::time::Duration::from_secs(60), 20)
            .peer_ip_key()
            .build();

        let middleware = RateLimiter::builder(backend.clone(), input).build();

        App::new()
            .app_data(global.clone())
            .wrap(middleware)
            .service(index)
            .service(verify_page)
            .service(recovery_page)
            .service(thanks_page)
            .service(tos_page)
            .service(pp_page)
            .service(contact_page)
            .service(reset)
            .service(webhook)
            .service(web::resource("/ws").route(web::get().to(pixels)))
            .service(Files::new("/", "./server/static/").index_file("index.html"))
        //.service(register)
        //.service(login)
    })
    .bind_rustls(
        ("0.0.0.0", 443),
        load_rustls_config()
    )?
    .run()
    .await
}
