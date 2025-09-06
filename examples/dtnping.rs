use anyhow::{bail, Result};
use clap::Parser;
use dtn7_plus::client::{DtnClient, DtnWsConnection};
use dtn7_plus::client::{Message, WsRecvData, WsSendData};
use humantime::parse_duration;
use rand::distr::Alphanumeric;
use rand::Rng;
use std::net::TcpStream;
use std::str::from_utf8;
use std::time::Duration;
use std::{convert::TryInto, io::Write};
use std::{thread, time};

fn get_random_payload(length: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}
#[derive(Debug, PartialEq)]
enum PingState {
    ReadyToSend,
    Receiving,
}

fn send_ping(
    wscon: &mut DtnWsConnection<TcpStream>,
    length: usize,
    src: &str,
    dst: &str,
) -> Result<()> {
    let payload = get_random_payload(length);
    let ping = WsSendData {
        src: src.to_owned(),
        dst: dst.to_owned(),
        delivery_notification: false,
        lifetime: 3600 * 24 * 1000,
        data: payload.as_bytes().to_vec(),
    };
    wscon.write_binary(serde_cbor::to_vec(&ping)?)
}

/// A simple Bundle Protocol 7 Ping Tool for Delay Tolerant Networking
#[derive(Parser, Debug)]
#[clap(version, author, long_about = None)]
struct Args {
    /// Local web port (default = 3000)
    #[clap(short, long, default_value_t = 3000)]
    port: u16,

    /// Use IPv6
    #[clap(short = '6', long)]
    ipv6: bool,

    /// Verbose output
    #[clap(short, long)]
    verbose: bool,

    /// Destination to ping
    #[clap(short, long)]
    dst: String,

    /// Payload size in bytes (default = 64)
    #[clap(short, long, default_value_t = 64)]
    size: usize,

    /// Number of pings to send
    #[clap(short, long, default_value_t = -1)]
    count: i64,

    /// Timeout to wait for reply (10s, 30m, 2h, ...)
    #[clap(short, long, default_value = "2000y")]
    timeout: String,
}
fn main() -> Result<()> {
    let args = Args::parse();

    let port = if let Ok(env_port) = std::env::var("DTN_WEB_PORT") {
        env_port // string is fine no need to parse number
    } else {
        args.port.to_string()
    };
    let localhost = if args.ipv6 { "[::1]" } else { "127.0.0.1" };

    let timeout: Duration = parse_duration(&args.timeout)?;

    let _dst_eid: bp7::EndpointID = args.dst.clone().try_into()?;

    let mut successful_pings = 0;

    let client = DtnClient::with_host_and_port(
        localhost.into(),
        port.parse::<u16>().expect("invalid port number"),
    );
    let endpoint: String = if client
        .local_node_id()
        .expect("failed to get local node id")
        .scheme()
        == "dtn"
    {
        "ping".into()
    } else {
        "7007".into()
    };
    client.register_application_endpoint(&endpoint)?;

    let stream = std::net::TcpStream::connect(format!("127.0.0.1:{}", port))?;
    stream.set_read_timeout(Some(timeout))?;
    let mut wscon = client.ws_custom(stream)?;
    //let mut wscon = client.ws()?;

    wscon.write_text("/data")?;
    let msg = wscon.read_text()?;
    if msg.starts_with("200 tx mode: data") {
        println!("[*] {}", msg);
    } else {
        bail!("[!] Failed to set mode to `data`");
    }

    wscon.write_text(&format!("/subscribe {}", endpoint))?;
    let msg = wscon.read_text()?;
    if msg.starts_with("200 subscribed") {
        println!("[*] {}", msg);
    } else {
        bail!("[!] Failed to subscribe to service");
    }
    let src = format!("{}{}", client.local_node_id()?, endpoint);

    let mut seq_num: u64 = 0;
    let mut state = PingState::ReadyToSend;
    let mut sent_time = time::Instant::now();
    println!("\nPING: {} -> {}", src, args.dst);
    loop {
        if state == PingState::ReadyToSend {
            if args.count > 0 && seq_num == args.count as u64 {
                break;
            }
            send_ping(&mut wscon, args.size, &src, &args.dst)?;
            seq_num += 1;
            sent_time = time::Instant::now();
            println!("[>] #{} size={}", seq_num, args.size);
            std::io::stdout().flush().unwrap();
            state = PingState::Receiving;
        }

        let msg_result = wscon.read_message();

        if let Err(err) = msg_result {
            let tunstenite_err = err.downcast::<tungstenite::Error>()?;

            if let tungstenite::error::Error::Io(io_err) = tunstenite_err {
                if io_err.kind() == std::io::ErrorKind::WouldBlock {
                    state = PingState::ReadyToSend;
                    println!("[!] *** timeout ***");
                    continue;
                } else {
                    anyhow::bail!(io_err);
                }
            } else {
                anyhow::bail!(tunstenite_err);
            }
        }
        let msg = msg_result.unwrap();
        match &msg {
            Message::Text(txt) => {
                if txt.starts_with("200") {
                    if args.verbose {
                        eprintln!("[<] {}", txt);
                    }
                } else {
                    println!("[!] {}", txt);
                }
            }
            Message::Binary(bin) => {
                let recv_data: WsRecvData =
                    serde_cbor::from_slice(bin).expect("Error decoding WsRecvData from server");

                println!("[<] #{} : {:?}", seq_num, sent_time.elapsed());
                successful_pings += 1;
                if args.verbose {
                    eprintln!(
                        "Bundle-Id: {} // From: {} / To: {}",
                        recv_data.bid, recv_data.src, recv_data.dst
                    );

                    if let Ok(data_str) = from_utf8(&recv_data.data) {
                        eprintln!("Data: {}", data_str);
                    }
                }

                thread::sleep(time::Duration::from_secs(1));
                state = PingState::ReadyToSend;
            }
            _ => {
                if args.verbose {
                    eprintln!("[<] Other: {:?}", msg);
                }
            }
        }
    }

    println!(
        "\n[*] {} of {} pings successful",
        successful_pings, args.count
    );

    if successful_pings < args.count {
        std::process::exit(1);
    }
    Ok(())
}
