use anyhow::{bail, Result};
use bp7::*;
use dtn7_plus::client::DtnClient;
use gumdrop::Options;
use std::convert::TryFrom;
use std::io::prelude::*;
use std::process::{self, Command};
use tempfile::NamedTempFile;
use tungstenite::Message;

/// A simple Bundle Protocol 7 Incoming Trigger Utility for Delay Tolerant Networking
#[derive(Debug, Options)]
struct CmdOptions {
    /// Print help message
    #[options(short = "h", long = "help")]
    help: bool,
    /// Verbose output
    #[options(short = "v", long = "verbose")]
    verbose: bool,
    /// Display version information
    #[options(short = "V", long = "version")]
    version: bool,
    /// Use IPv6
    #[options(short = "6", long = "ipv6")]
    ipv6: bool,
    /// Local web port
    #[options(short = "p", long = "port", default = "3000")]
    port: u16,
    /// Specify local endpoint, e.g. '/incoming', or a group endpoint 'dtn://helpers/~incoming'
    #[options(short = "e", long = "endpoint", required)]
    endpoint: String,
    /// Command to execute for incoming bundles, param1 = source, param2 = payload file
    #[options(short = "c", long = "command", required)]
    command: String,
}

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
    let opts = CmdOptions::parse_args_default_or_exit();

    if opts.help {
        println!("{}", CmdOptions::usage());
        process::exit(0);
    }
    if opts.version {
        println!("{}", dtn7::VERSION);
        process::exit(0);
    }

    let verbose: bool = opts.verbose;
    let port = std::env::var("DTN_WEB_PORT").unwrap_or_else(|_| opts.port.to_string());

    let localhost = if opts.ipv6 { "[::1]" } else { "127.0.0.1" };

    let client = DtnClient::with_host_and_port(
        localhost.into(),
        port.parse::<u16>().expect("invalid port number"),
    );

    let endpoint: String = opts.endpoint;
    let command: String = opts.command;

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
