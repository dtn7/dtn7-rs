use bp7::*;
use clap::{crate_authors, crate_version, App, Arg};
use dtn7_plus::client::DtnClient;
use std::convert::TryFrom;
use std::io::prelude::*;
use std::process::Command;
use tempfile::NamedTempFile;
use ws::{Builder, CloseCode, Handler, Handshake, Message, Result, Sender};

struct Connection {
    endpoint: String,
    out: Sender,
    subscribed: bool,
    verbose: bool,
    command: String,
}

impl Connection {
    fn write_temp_file(&self, data: &[u8]) -> Result<NamedTempFile> {
        let mut data_file = NamedTempFile::new()?;
        data_file.write_all(data)?;
        data_file.flush()?;
        let fname_param = format!("{}", data_file.path().display());
        if self.verbose {
            eprintln!("data file: {}", fname_param);
        }
        Ok(data_file)
    }
    fn execute_cmd(&self, data_file: NamedTempFile, bndl: &Bundle) -> Result<()> {
        let fname_param = format!("{}", data_file.path().display());
        let cmd_args = &mut self.command.split_whitespace();
        let mut command = Command::new(cmd_args.next().unwrap()); //empty string handled by clap
        while let Some(arg) = cmd_args.next() {
            command.arg(arg);
        }
        let output = command
            .arg(bndl.primary.source.to_string())
            .arg(fname_param)
            .output()
            .unwrap_or_else(|e| {
                eprintln!("Error executing command: {}", e);
                std::process::exit(1);
            });

        if !output.status.success() || self.verbose {
            println!("status: {}", output.status);
            std::io::stdout().write_all(&output.stdout)?;
            std::io::stderr().write_all(&output.stderr)?;
        }
        Ok(())
    }
}

impl Handler for Connection {
    fn on_open(&mut self, _: Handshake) -> Result<()> {
        self.out.send(format!("/bundle"))?;
        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> Result<()> {
        match msg {
            Message::Text(txt) => {
                if txt.starts_with("200") {
                    if txt == "200 subscribed" {
                        if self.verbose {
                            eprintln!("successfully subscribed to {}!", self.endpoint);
                        }
                        self.subscribed = true;
                    } else if txt == "200 tx mode: bundle" {
                        if self.verbose {
                            eprintln!("successfully set mode: bundle!");
                        }
                        self.out.send(format!("/subscribe {}", self.endpoint))?;
                    }
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
                            eprintln!("Bundle-Id: {}", bndl.id());
                        }
                        let data_file = self.write_temp_file(data)?;
                        self.execute_cmd(data_file, &bndl)?;
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
    let matches = App::new("dtntrigger")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple Bundle Protocol 7 Incoming Trigger Utility for Delay Tolerant Networking")
        .arg(
            Arg::with_name("endpoint")
                .short("e")
                .long("endpoint")
                .value_name("ENDPOINT")
                .help("Specify local endpoint, e.g. '/incoming', or a group endpoint 'dtn://helpers/incoming'")
                .takes_value(true),
        )
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
            Arg::with_name("cmd")
                .short("c")
                .long("command")
                .value_name("CMD")
                .help("Command to execute for incoming bundles, param1 = source, param2 = payload file")
                .required(true)
                .takes_value(true)
                .empty_values(false),
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

    let endpoint: String = matches.value_of("endpoint").unwrap().into();
    let command: String = matches.value_of("cmd").unwrap().into();

    client.register_application_endpoint(&endpoint)?;
    let mut ws = Builder::new()
        .build(|out| Connection {
            endpoint: endpoint.clone(),
            out,
            subscribed: false,
            verbose,
            command: command.clone(),
        })
        .unwrap();
    ws.connect(url::Url::parse(&local_url)?)?;
    ws.run()?;
    Ok(())
}
