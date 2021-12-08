use anyhow::{bail, Result};
use clap::{crate_authors, crate_version, App, Arg};
use dtn7_plus::client::{DtnClient, DtnWsConnection};
use dtn7_plus::client::{Message, WsRecvData, WsSendData};
use humantime::parse_duration;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::str::from_utf8;
use std::time::Duration;
use std::{convert::TryInto, io::Write};
use std::{env, net::TcpStream};
use std::{thread, time};

fn get_random_payload(length: usize) -> String {
    thread_rng()
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
    wscon.write_binary(&serde_cbor::to_vec(&ping)?)
}
fn main() -> Result<()> {
    let matches = App::new("dtnping")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple Bundle Protocol 7 Ping Tool for Delay Tolerant Networking")
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .value_name("PORT")
                .help("Local web port (default = 3000)")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .help("verbose output")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("ipv6")
                .short("6")
                .long("ipv6")
                .help("Use IPv6")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("destination")
                .short("d")
                .long("destination")
                .help("Destination to ping")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("payloadsize")
                .short("s")
                .long("size")
                .help("Payload size")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("count")
                .short("c")
                .long("count")
                .help("Number of pings to send")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("timeout")
                .short("t")
                .long("timeout")
                .help("Time to wait for reply (10s, 30m, 2h, ...)")
                .takes_value(true),
        )
        .get_matches();

    let verbose: bool = matches.is_present("verbose");
    let port = std::env::var("DTN_WEB_PORT").unwrap_or_else(|_| "3000".into());
    let port = matches.value_of("port").unwrap_or(&port); // string is fine no need to parse number
    let localhost = if matches.is_present("ipv6") {
        "[::1]"
    } else {
        "127.0.0.1"
    };
    let payload_size: usize = matches.value_of("payloadsize").unwrap_or("64").parse()?;
    let count: i32 = matches.value_of("count").unwrap_or("-1").parse()?;
    let timeout: Duration = parse_duration(matches.value_of("timeout").unwrap_or("2000y"))?;

    let dst = matches.value_of("destination").unwrap();

    let _dst_eid: bp7::EndpointID = dst.try_into()?;

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

    let stream = std::net::TcpStream::connect(&format!("127.0.0.1:{}", port))?;
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
    let src = format!("{}/{}", client.local_node_id()?, endpoint);

    let mut seq_num: u64 = 0;
    let mut state = PingState::ReadyToSend;
    let mut sent_time = time::Instant::now();
    println!("\nPING: {} -> {}", src, dst);
    loop {
        if state == PingState::ReadyToSend {
            if count > 0 && seq_num == count as u64 {
                break;
            }
            send_ping(&mut wscon, payload_size, &src, dst)?;
            seq_num += 1;
            sent_time = time::Instant::now();
            println!("[>] #{} size={}", seq_num, payload_size);
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
                    if verbose {
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
                if verbose {
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
                if verbose {
                    eprintln!("[<] Other: {:?}", msg);
                }
            }
        }
    }

    println!("\n[*] {} of {} pings successful", successful_pings, count);

    if successful_pings < count {
        std::process::exit(1);
    }
    Ok(())
}
