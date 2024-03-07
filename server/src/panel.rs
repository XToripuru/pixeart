use crate::{
    blacklist::Blacklist,
    server::{ServerHandler, Tier, User},
    db::{Record, LogsRecord},
    DB
};
use shared::Message;
use std::{path::Path, sync::Arc, error::Error};
use sysinfo::{CpuExt, CpuRefreshKind, NetworkExt, RefreshKind, System, SystemExt};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    spawn,
    sync::Mutex,
    time::interval,
    select
};
use log::debug;

#[derive(Clone)]
pub struct Panel {
    blacklist: Blacklist,
    server: ServerHandler,
}

impl Panel {
    pub fn new(blacklist: Blacklist, server: ServerHandler) -> Self {
        Panel { blacklist, server }
    }
    pub async fn run(self) {
        let listener = TcpListener::bind("0.0.0.0:5555").await.unwrap();

        let refresh = RefreshKind::new()
            .with_memory()
            .with_cpu(CpuRefreshKind::everything())
            .with_networks()
            .with_networks_list();

        let system = Arc::new(Mutex::new(System::new_with_specifics(refresh.clone())));

        loop {
            let Ok((stream, addr)) = listener.accept().await else { continue; };

            println!("{} connected to admin panel", addr);

            let _: Option<Record> = match DB.create("logs")
            .content(LogsRecord::new(addr.to_string(), format!("Connected to admin panel")))
            .await 
            {
                Ok(x) => x,
                Err(_) => None
            };

            spawn(Panel::handle_connection(
                self.clone(),
                stream,
                system.clone(),
                refresh.clone(),
            ));
        }
    }
    async fn handle_connection(
        self,
        mut stream: TcpStream,
        system: Arc<Mutex<System>>,
        refresh: RefreshKind,
    ) {
        let mut logged = false;
        let mut interval = interval(std::time::Duration::from_secs(5));

        let mut header = [0u8; 8];
        let mut message = [0u8; 2048];

        loop {
            select! {
                _ = interval.tick() => {
                    if !logged {
                        continue;
                    }
                    
                    let mut lock = system.lock().await;
                    lock.refresh_specifics(refresh);

                    let cpus = lock
                                .cpus()
                                .iter()
                                .map(|cpu| (cpu.name().to_owned(), cpu.cpu_usage()))
                                .collect::<Vec<_>>();

                    let memory = (lock.used_memory(), lock.total_memory()); //memory
                    let data = lock
                        .networks()
                        .into_iter()
                        .map(|(name, data)| {
                            (name.clone(), data.received(), data.transmitted())
                        })
                        .collect::<Vec<_>>();

                    let _ = send_message(&mut stream, Message::Usage((cpus, memory, data))).await;
                }
                Ok(n) = stream.read(&mut header) => {
                    if n == 0 {
                        break;
                    }

                    let mlen = usize::from_le_bytes(header);

                    match stream.read_exact(&mut message[0..mlen]).await {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(err) => {}
                    }

                    let msg: Message = bincode::deserialize(&message[0..mlen]).unwrap();

                    match msg {
                        Message::Login(password) => {
                            logged = password == shared::secret::PANEL_PASSWORD;
                        }
                        Message::Ban(ip) if logged => {
                            self.blacklist.ban(ip).await;
                        }
                        Message::Unban(ip) if logged => {
                            self.blacklist.unban(ip).await;
                        }
                        Message::SetTier(name, tier) if logged => {
                            debug!("Setting tier {tier} for {name}");
                            
                            let Some(tier) = Tier::from_numeric(tier) else { continue; };
                            let _: Option<User> = match DB.query("UPDATE type::thing(type::table($table), type::string($user)) SET tier=$tier")
                                .bind(("table", "users"))
                                .bind(("user", &*name))
                                .bind(("tier", tier))
                                .await {
                                    Ok(mut res) => res.take(0).unwrap_or(None),
                                    Err(err) =>  continue
                                };
                        }
                        Message::Logs(timestamp) if logged => {
                            let res: Vec<LogsRecord> = match DB.query("SELECT * FROM type::table($table) WHERE timestamp>=$timestamp ORDER BY timestamp ASC")
                                .bind(("table", "logs"))
                                .bind(("timestamp", timestamp))
                                .await {
                                    Ok(mut res) => res.take(0).unwrap_or(vec![]),
                                    Err(err) => continue
                                };
                            
                            let logs = res.into_iter().map(|log| (log.ip, log.msg, log.timestamp)).collect();

                            let _ = send_message(&mut stream, Message::LogsResponse(logs)).await;
                        }
                        Message::SaveGrid if logged => {
                            self.server.save_grid();
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

async fn send_message(stream: &mut TcpStream, msg: Message) -> std::io::Result<()> {
    let response = bincode::serialize(&msg).unwrap();

    let length = response.len().to_le_bytes();

    stream.write_all(&length).await?;
    stream.write_all(&response).await?;

    Ok(())
}
