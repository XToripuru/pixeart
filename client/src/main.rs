use std::{net::{IpAddr, TcpStream}, str::FromStr, io::{Read,Write}, sync::mpsc::channel, fs};
use shared::{Message, secret::PANEL_PASSWORD};
use chrono::{offset::TimeZone, Utc};

fn main() {
    let (tx, rx) = channel::<Message>();
    let mut stream = TcpStream::connect("192.168.0.106:5555").unwrap();

    send_message(&mut stream, Message::Login(String::from(PANEL_PASSWORD))).unwrap();

    // user input
    std::thread::spawn(move || {
        let mut input = String::new();
        
        loop {
            input.clear();
            std::io::stdin().read_line(&mut input).unwrap();
            let trimmed = input.trim();

            let mut args = trimmed.split(" ");

            let msg = match args.next().unwrap() {
                "ban" => {
                    let Some(who) = args.next() else { println!("Specify who to ban"); continue; };
                    Message::Ban(IpAddr::from_str(who).unwrap())
                }
                "unban" => {
                    let Some(who) = args.next() else { println!("Specify who to unban"); continue; };
                    Message::Unban(IpAddr::from_str(who).unwrap())
                }
                "logs" => {
                    let Some(timestamp) = args.next() else { println!("Specify timestamp"); continue; };
                    Message::Logs(timestamp.parse().unwrap())
                }
                "set-tier" => {
                    let Some(who) = args.next() else { println!("Specify who to give this tier"); continue; };
                    let Some(tier) = args.next() else { println!("Specify what tier to give"); continue; };

                    Message::SetTier(who.into(), tier.parse().unwrap())
                }
                "save" => Message::SaveGrid,
                _ => {
                    input.clear();
                    continue;
                }
            };

            tx.send(msg).unwrap();
        }

    });

    let mut header = [0u8; 8];
    let mut message = [0u8; 10000];

    loop {
        match stream.read(&mut header) {
            Ok(0) => {
                println!("breaking1");
                break
            }
            Err(_) => continue,
            Ok(_n) => {
                let mlen = usize::from_le_bytes(header);
                    match stream.read_exact(&mut message[0..mlen]) {
                        Ok(_) => {}
                        Err(_err) => {
                            println!("breaking2");
                            break
                        }
                    }

                    let msg: Message = bincode::deserialize(&message[0..mlen]).unwrap();

                    match msg {
                        Message::Usage((cpus, memory, network)) => {
                            // clear console
                            print!("{esc}c", esc = 27 as char);

                            for (idx, (name, usage)) in cpus.iter().enumerate() {
                                if idx % 3 == 0 {
                                    print!("\n");
                                }

                                print!("{} usage: {:.2}%\t", name, usage);
                            }
                            print!("\n\n");

                            println!("RAM: {:.2}/{:.2} GB", memory.0 as f32 / 1024_f32.powi(3), memory.1 as f32 / 1024_f32.powi(3));

                            for (name, sent, recv) in network {
                                println!("{}: {} KB | {} KB", name, sent / 1024, recv / 1024);
                            }
                        },
                        Message::LogsResponse(logs) => {
                            let mut result = String::new();

                            for (ip, msg, timestamp) in logs {
                                let date = Utc.timestamp_opt(timestamp, 0)
                                    .unwrap()
                                    .format("%d/%m/%Y %H:%M:%S");

                                result.push_str(&format!("[{} :: {}] - {} \n", date, ip, msg));
                            }

                            fs::write("logs", result).unwrap();
                        }
                        _ => {}
                    }
            }
        }
        if let Ok(msg) = rx.try_recv() {
            send_message(&mut stream, msg).unwrap();
        }
    }
}

fn send_message(stream: &mut TcpStream, msg: Message) -> std::io::Result<()> {
    let response = bincode::serialize(&msg).unwrap();

    let length = response.len().to_le_bytes();

    stream.write_all(&length)?;
    stream.write_all(&response)?;

    Ok(())
}