use bp7::*;
use clap::{crate_authors, crate_version, App, Arg};
use dtn7_plus::client::DtnClient;
use dtn7_plus::client::WsSendData;
use std::convert::TryFrom;
use std::io::{self, Write};
use std::str::from_utf8;
use ws::{Builder, CloseCode, Handler, Handshake, Message, Result, Sender};

struct Connection {
    endpoint: String,
    out: Sender,
    subscribed: bool,
    verbose: bool,
}

impl Handler for Connection {
    fn on_open(&mut self, _: Handshake) -> Result<()> {
        self.out.send(format!("/subscribe {}", self.endpoint))?;
        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> Result<()> {
        match msg {
            Message::Text(txt) => {
                if txt == "subscribed" {
                    self.subscribed = true;
                    if self.verbose {
                        println!("Subscribed to endpoint {}", self.endpoint);
                    }
                } else if txt.starts_with("200") {
                } else {
                    eprintln!("Unexpected response: {}", txt);
                    self.out.close(CloseCode::Error)?;
                }
            }
            Message::Binary(bin) => {
                let bndl: Bundle =
                    Bundle::try_from(bin).expect("Error decoding bundle from server");
                if bndl.is_administrative_record() {
                    eprintln!("Handling of administrative records not yet implemented!");
                } else {
                    if let Some(data) = bndl.payload() {
                        if self.verbose {
                            eprintln!(
                                "Bundle-Id: {} // From: {} / To: {}",
                                bndl.id(),
                                bndl.primary.source,
                                bndl.primary.destination
                            );

                            if let Ok(data_str) = from_utf8(&data) {
                                eprintln!("Data: {}", data_str);
                            }
                        } else {
                            print!(".");
                            std::io::stdout().flush().unwrap();
                        }
                        // flip src and destionation
                        let src = bndl.primary.destination.clone();
                        let dst = bndl.primary.source.clone();
                        // construct response with copied payload
                        let echo_response = WsSendData {
                            src,
                            dst,
                            delivery_notification: false,
                            lifetime: bndl.primary.lifetime,
                            data: data.clone(),
                        };
                        self.out
                            .send(serde_cbor::to_vec(&echo_response).unwrap())
                            .expect("error sending echo response");
                    } else {
                        if self.verbose {
                            eprintln!("Unexpected payload!");
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
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
    let local_url = format!("ws://{}:{}/ws", localhost, port);

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

    let mut ws = Builder::new()
        .build(|out| Connection {
            endpoint: endpoint.clone(),
            out,
            subscribed: false,
            verbose,
        })
        .unwrap();
    ws.connect(url::Url::parse(&local_url)?)?;
    ws.run()?;
    Ok(())
}
