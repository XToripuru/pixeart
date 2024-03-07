use crate::{
    handler::Message,
    DB,
    db::{Record, Pixel, LogsRecord}
};
use std::{net::{IpAddr, SocketAddr}, path::Path, sync::Arc, time::Duration, fmt::Display};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::{
    fs, select, spawn,
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    time::{interval, sleep},
};
use log::debug;

pub const GRID_WIDTH: i32 = 1000;
pub const GRID_HEIGHT: i32 = 1000;
const QUEUE_SEND: Duration = Duration::from_millis(500);
const GRID_SAVE: Duration = Duration::from_secs(60 * 30);

#[derive(Debug)]
pub enum ServerError {
    PixelCooldown,
}

enum Command {
    Connect(
        State,
        SocketAddr,
        UnboundedSender<Message>,
        oneshot::Sender<Option<(i64, Vec<u8>)>>,
    ),
    Disconnect(i64),
    Upgrade(i64, User, oneshot::Sender<Option<(Tier, i64)>>),
    Downgrade(i64),
    Update(i64, PixelUpdate, oneshot::Sender<Result<(), ServerError>>), // updated pixel
    Verify(String),
    TierChange(String, Tier),
    SaveGrid
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Tier {
    Free,
    Tier(u8),
}

impl Tier {
    pub fn from_numeric(value: u8) -> Option<Tier> {
        match value {
            0 => Some(Tier::Free),
            1..=5 => Some(Tier::Tier(value)),
            _ => None,
        }
    }
    pub fn into_numeric(self) -> u8 {
        match self {
            Tier::Free => 0,
            Tier::Tier(n) => n,
        }
    }
    pub fn price(self) -> u8 {
        match self {
            Tier::Free => 0,
            Tier::Tier(tier) => match tier {
                1 => 2,
                2 => 4,
                3 => 8,
                4 => 20,
                5 => 60,
                _ => unreachable!()
            }
        }
    }
    fn cooldown(self) -> i64 {
        match self {
            Tier::Free => 180,
            Tier::Tier(tier) => match tier {
                1 => 60,
                2 => 30,
                3 => 15,
                4 => 5,
                5 => 1,
                _ => unreachable!()
            },
        }
    }
}


impl PartialOrd for Tier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.into_numeric().cmp(&other.into_numeric()))
    }
}

impl Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tier::Free => f.write_str("Free"),
            Tier::Tier(tier) => match tier {
                1 => f.write_str("Tier I"),
                2 => f.write_str("Tier II"),
                3 => f.write_str("Tier III"),
                4 => f.write_str("Tier IV"),
                5 => f.write_str("Tier V"),
                _ => f.write_str("Unknown Tier"),
            }
        }
    }
}

struct Connection {
    id: i64,
    ip: SocketAddr,
    socket: UnboundedSender<Message>,
}

impl Connection {
    fn new(id: i64, ip: SocketAddr, socket: UnboundedSender<Message>) -> Self {
        Connection { id, ip, socket }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct User {
    pub name: String,
    pub email: String,
    pub password: [u8; 32],
    pub tier: Tier,
    pub last: i64,
    pub verified: bool,
}

impl User {
    pub fn new(name: String, email: String, password: [u8; 32]) -> Self {
        User {
            name,
            email,
            password,
            tier: Tier::Free,
            last: 0,
            verified: false,
        }
    }
}

#[derive(Clone)]
pub enum State {
    Unverified,
    Verified(User),
}

struct Link {
    user: State,
    conn: Connection,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PixelUpdate([u8; 3], usize);

pub struct Server {
    users: Vec<Link>,
    queue: Vec<PixelUpdate>,
    grid: Vec<u8>,
    rx: UnboundedReceiver<Command>,
    next_id: i64,
}

impl Server {
    pub async fn new() -> (Self, ServerHandler) {
        let (tx, rx) = unbounded_channel::<Command>();
        let grid = fs::read("image").await.unwrap();

        (
            Server {
                users: vec![],
                queue: vec![],
                rx,
                grid,
                next_id: 0,
            },
            ServerHandler { cmd_tx: tx },
        )
    }
    pub async fn run(mut self) {
        let mut grid_interval = interval(GRID_SAVE);
        let mut queue_interval = interval(QUEUE_SEND);

        loop {
            select! {
                _ = grid_interval.tick() => {
                    let grid = self.grid.clone();
                    spawn(save_grid(grid));
                }
                _ = queue_interval.tick() => {
                    if self.queue.len() > 0 {
                        let updates: Arc<[PixelUpdate]> = Arc::from(&self.queue[..]);

                        for link in &self.users {
                            debug!("Sending queue to {}", link.conn.ip);
                            let _ = link.conn.socket.send(Message::Queue(Arc::clone(&updates)));
                        }

                        self.queue.clear();
                    }
                }
                Some(cmd) = self.rx.recv() => {
                    match cmd {
                        Command::SaveGrid => {
                            let grid = self.grid.clone();
                            spawn(save_grid(grid));
                        }
                        Command::Connect(user, ip, socket, res_tx) => {
                            if self
                                .users
                                .iter()
                                .find(|link| link.conn.ip.ip() == ip.ip())
                                .is_some()
                            {
                                let _ = res_tx.send(None);
                                continue;
                            }

                            debug!("{} connected", ip);
                            self.users.push(Link {
                                user,
                                conn: Connection::new(self.next_id, ip, socket),
                            });

                            let _ = res_tx.send(Some((self.next_id, Vec::from(&self.grid[..]))));
                            self.next_id += 1;
                        }
                        Command::Disconnect(conn_id) => {
                            for i in 0..self.users.len() {
                                if self.users[i].conn.id == conn_id {
                                    debug!("{} disconnected", self.users[i].conn.ip);

                                    let link = self.users.swap_remove(i);
                                    if let State::Verified(user) = link.user {
                                        spawn(async move {
                                            let mut tries = 10;
                                            while let Err(_) = save_user(&user).await {
                                                if tries <= 0 {
                                                    break;
                                                }
                                                tries -= 1;

                                                sleep(Duration::from_millis(500)).await;
                                            }
                                        });
                                    }

                                    break;
                                }
                            }
                        }
                        Command::Upgrade(conn_id, mut user, res_tx) => {
                            let mut response = None;

                            for i in 0..self.users.len() {
                                let State::Verified(ref _user) = self.users[i].user else { continue };

                                if _user.name == user.name {
                                    // so we dont lose progress
                                    user = _user.clone();
                                    response = Some((user.tier.clone(), user.last));
                                    let _ = self.users[i].conn.socket.send(Message::AnotherSession);

                                    self.users[i].user = State::Unverified;
                                    break;
                                }
                            }

                            let Some(link) = self
                                .users
                                .iter_mut()
                                .find(|link| link.conn.id == conn_id) else { continue; };
                                
                            debug!("{} logged in", link.conn.ip);

                            link.user = State::Verified(user);
                            let _ = res_tx.send(response);
                        }
                        Command::Downgrade(conn_id) => {
                            let Some(link) = self
                                .users
                                .iter_mut()
                                .find(|link| link.conn.id == conn_id) else { continue; };
                                
                            debug!("{} logged out", link.conn.ip);

                            if let State::Verified(user) = std::mem::replace(&mut link.user, State::Unverified) {
                                spawn(async move {
                                    let mut tries = 10;
                                    while let Err(_) = save_user(&user).await {
                                        if tries <= 0 {
                                            break;
                                        }
                                        tries -= 1;

                                        sleep(Duration::from_millis(500)).await;
                                    }
                                });
                            }

                        }
                        Command::Update(conn_id, PixelUpdate(color, idx), res_tx) => {
                            let Some(link) = self
                                .users
                                .iter_mut()
                                .find(|link| link.conn.id == conn_id) else { continue; };

                            let State::Verified(ref mut user) = link.user else { continue };
                            let now  = Utc::now().timestamp();

                            if now - user.last < user.tier.cooldown() {
                                let _ = res_tx.send(Err(ServerError::PixelCooldown));
                                continue;
                            }

                            user.last = now;
                            debug!("{} updating pixel at {}", link.conn.ip, idx);

                            for i in 0..3 {
                                self.grid[3 * idx + i] = color[i];
                            }

                            let _ = res_tx.send(Ok(()));
                            self.queue.push(PixelUpdate(color, idx));
                        }
                        Command::Verify(email) => {
                            for link in &mut self.users {
                                let State::Verified(ref mut user) = link.user else { continue };

                                if user.email == email {
                                    user.verified = true;

                                    let _ = link.conn.socket.send(Message::Verified);
                                }
                            }
                        }
                        Command::TierChange(email, tier) => {
                            for link in &mut self.users {
                                let State::Verified(ref mut user) = link.user else { continue };

                                if user.email == email {
                                    user.tier = tier;

                                    let _ = link.conn.socket.send(Message::TierChange(tier));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn save_grid(grid: Vec<u8>) {
    let mut tries = 0;
    while let Err(_) = fs::write("image.temp", &grid).await {
        if tries > 9 {
            let _: Option<Record> = match DB.create("logs")
                .content(LogsRecord::new(IpAddr::from([127u8, 0, 0, 1]).to_string(), String::from("Image file could not be created")))
                .await
            {
                Ok(x) => x,
                Err(_) => None
            };
            break;
        }
        tries += 1;

        sleep(Duration::from_secs(5)).await;
    }
    fs::rename("image.temp", "image").await.unwrap();
}

async fn save_user(user: &User) -> surrealdb::Result<()> {
    if user.verified {
        let _: Option<User> = DB.update(("users", &*user.email)).content(user).await?;
    }

    Ok(())
}


#[derive(Clone)]
pub struct ServerHandler {
    cmd_tx: UnboundedSender<Command>,
}

impl ServerHandler {
    pub async fn connect(
        &self,
        tx: UnboundedSender<Message>,
        info: State,
        ip: SocketAddr,
    ) -> Option<(i64, Vec<u8>)> {
        let (res_tx, rx) = oneshot::channel::<Option<(i64, Vec<u8>)>>();

        self.cmd_tx
            .send(Command::Connect(info, ip, tx, res_tx))
            .unwrap();

        rx.await.unwrap()
    }
    pub fn disconnect(&self, conn_id: i64) {
        self.cmd_tx.send(Command::Disconnect(conn_id)).unwrap();
    }
    pub async fn upgrade(&self, conn_id: i64, user: User) -> Option<(Tier, i64)> {
        let (res_tx, rx) = oneshot::channel::<Option<(Tier, i64)>>();

        self.cmd_tx
            .send(Command::Upgrade(conn_id, user, res_tx))
            .unwrap();

        rx.await.unwrap()
    }
    pub fn downgrade(&self, conn_id: i64) {
        self.cmd_tx.send(Command::Downgrade(conn_id)).unwrap();
    }
    pub async fn update_pixel(
        &self,
        conn_id: i64,
        color: [u8; 3],
        idx: usize,
    ) -> Result<(), ServerError> {
        let (res_tx, rx) = oneshot::channel::<Result<(), ServerError>>();

        self.cmd_tx
            .send(Command::Update(conn_id, PixelUpdate(color, idx), res_tx))
            .unwrap();

        rx.await.unwrap()
    }
    pub async fn verify(&self, email: String) {
        self.cmd_tx.send(Command::Verify(email)).unwrap();
    }
    pub async fn tier_change(&self, email: String, tier: Tier) {
        self.cmd_tx.send(Command::TierChange(email, tier)).unwrap();
    }
    pub fn save_grid(&self) {
        self.cmd_tx.send(Command::SaveGrid).unwrap();
    }
}
