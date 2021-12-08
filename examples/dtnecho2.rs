use anyhow::{bail, Result};
use clap::{crate_authors, crate_version, App, Arg};
use dtn7_plus::client::DtnClient;
use dtn7_plus::client::{Message, WsRecvData, WsSendData};
use std::env;
use std::io::Write;
use std::str::from_utf8;
use std::time::Instant;

fn main() -> Result<()> {
    let matches = App::new("dtnecho")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple Bundle Protocol 7 Echo Service for Delay Tolerant Networking")
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
        .get_matches();

    let verbose: bool = matches.is_present("verbose");
    let port = std::env::var("DTN_WEB_PORT").unwrap_or_else(|_| "3000".into());
    let port = matches.value_of("port").unwrap_or(&port); // string is fine no need to parse number
    let localhost = if matches.is_present("ipv6") {
        "[::1]"
    } else {
        "127.0.0.1"
    };

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
        "echo".into()
    } else {
        "7".into()
    };
    client.register_application_endpoint(&endpoint)?;

    let mut wscon = client.ws()?;

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

    loop {
        let msg = wscon.read_message()?;
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
                let now = Instant::now();
                let recv_data: WsRecvData =
                    serde_cbor::from_slice(bin).expect("Error decoding WsRecvData from server");

                if verbose {
                    eprintln!(
                        "Bundle-Id: {} // From: {} / To: {}",
                        recv_data.bid, recv_data.src, recv_data.dst
                    );

                    if let Ok(data_str) = from_utf8(&recv_data.data) {
                        eprintln!("Data: {}", data_str);
                    }
                } else {
                    print!(".");
                    std::io::stdout().flush().unwrap();
                }
                // flip src and destionation
                let src = recv_data.dst.to_owned();
                let dst = recv_data.src.to_owned();
                // construct response with copied payload
                let echo_response = WsSendData {
                    src,
                    dst,
                    delivery_notification: false,
                    lifetime: 3600 * 24 * 1000,
                    data: recv_data.data,
                };
                wscon
                    .write_binary(&serde_cbor::to_vec(&echo_response)?)
                    .expect("error sending echo response");
                if verbose {
                    println!("Processing bundle took {:?}", now.elapsed());
                }
            }
            _ => {
                if verbose {
                    eprintln!("[<] Other: {:?}", msg);
                }
            }
        }
    }
}
