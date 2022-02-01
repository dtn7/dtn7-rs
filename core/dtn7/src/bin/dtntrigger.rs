use anyhow::{bail, Result};
use bp7::*;
use clap::{crate_authors, crate_version, App, Arg};
use dtn7_plus::client::DtnClient;
use std::convert::TryFrom;
use std::io::prelude::*;
use std::process::Command;
use tempfile::NamedTempFile;
use tungstenite::Message;

fn write_temp_file(data: &[u8], verbose: bool) -> Result<NamedTempFile> {
    let mut data_file = NamedTempFile::new()?;
    data_file.write_all(data)?;
    data_file.flush()?;
    let fname_param = format!("{}", data_file.path().display());
    if verbose {
        eprintln!("[*] data file: {}", fname_param);
    }
    Ok(data_file)
}
fn execute_cmd(
    command: &str,
    data_file: NamedTempFile,
    bndl: &Bundle,
    verbose: bool,
) -> Result<()> {
    let fname_param = format!("{}", data_file.path().display());
    let cmd_args = &mut command.split_whitespace();
    let mut command = Command::new(cmd_args.next().unwrap()); //empty string handled by clap
    for arg in cmd_args {
        command.arg(arg);
    }
    let output = command
        .arg(bndl.primary.source.to_string())
        .arg(fname_param)
        .output()
        .unwrap_or_else(|e| {
            eprintln!("[!] Error executing command: {}", e);
            std::process::exit(1);
        });

    if !output.status.success() || verbose {
        println!("[*] status: {}", output.status);
        std::io::stdout().write_all(&output.stdout)?;
        std::io::stderr().write_all(&output.stderr)?;
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let matches = App::new("dtntrigger")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple Bundle Protocol 7 Incoming Trigger Utility for Delay Tolerant Networking")
        .arg(
            Arg::new("endpoint")
                .short('e')
                .long("endpoint")
                .value_name("ENDPOINT")
                .help("Specify local endpoint, e.g. '/incoming', or a group endpoint 'dtn://helpers/~incoming'")
                .takes_value(true),
        )
        .arg(
            Arg::new("port")
                .short('p')
                .long("port")
                .value_name("PORT")
                .help("Local web port (default = 3000)")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::new("cmd")
                .short('c')
                .long("command")
                .value_name("CMD")
                .help("Command to execute for incoming bundles, param1 = source, param2 = payload file")
                .required(true)
                .takes_value(true)
                .forbid_empty_values(true),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("verbose output")
                .takes_value(false),
        )
        .arg(
            Arg::new("ipv6")
                .short('6')
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

    let endpoint: String = matches.value_of("endpoint").unwrap().into();
    let command: String = matches.value_of("cmd").unwrap().into();

    client.register_application_endpoint(&endpoint)?;
    let mut wscon = client.ws()?;

    wscon.write_text("/bundle")?;
    let msg = wscon.read_text()?;
    if msg.starts_with("200 tx mode: bundle") {
        println!("[*] {}", msg);
    } else {
        bail!("[!] Failed to set mode to `bundle`");
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
        match msg {
            Message::Text(txt) => {
                eprintln!("Unexpected response: {}", txt);
                break;
            }
            Message::Binary(bin) => {
                let bndl: Bundle =
                    Bundle::try_from(bin).expect("Error decoding bundle from server");
                if bndl.is_administrative_record() {
                    eprintln!("[!] Handling of administrative records not yet implemented!");
                } else if let Some(data) = bndl.payload() {
                    if verbose {
                        eprintln!("[<] Received Bundle-Id: {}", bndl.id());
                    }
                    let data_file = write_temp_file(data, verbose)?;
                    execute_cmd(&command, data_file, &bndl, verbose)?;
                } else if verbose {
                    eprintln!("[!] Unexpected payload!");
                    break;
                }
            }
            Message::Ping(_) => {
                if verbose {
                    eprintln!("[<] Ping")
                }
            }
            Message::Pong(_) => {
                if verbose {
                    eprintln!("[<] Ping")
                }
            }
            Message::Close(_) => {
                if verbose {
                    eprintln!("[<] Close")
                }
                break;
            }
        }
    }
    Ok(())
}
