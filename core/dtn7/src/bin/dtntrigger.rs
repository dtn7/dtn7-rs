use anyhow::{Result, bail};
use bp7::*;
use clap::{Parser, crate_authors, crate_version};
use dtn7_plus::client::DtnClient;
use std::convert::TryFrom;
use std::io::prelude::*;
use std::process::Command;
use tempfile::NamedTempFile;
use tungstenite::Message;
use tungstenite::protocol::WebSocketConfig;

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
        eprintln!("[*] status: {}", output.status);
        std::io::stdout().write_all(&output.stdout)?;
        std::io::stderr().write_all(&output.stderr)?;
    }
    Ok(())
}

/// A simple Bundle Protocol 7 Incoming Trigger Utility for Delay Tolerant Networking
#[derive(Parser, Debug)]
#[clap(version = crate_version!(), author = crate_authors!(), about, long_about = None)]
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

    /// Specify local endpoint, e.g. 'incoming', or a group endpoint 'dtn://helpers/~incoming'
    #[clap(short, long)]
    endpoint: String,

    /// Just print the message
    #[clap(long)]
    print: bool,

    /// Command to execute for incoming bundles, param1 = source, param2 = payload file
    #[clap(short, long, default_value = "echo")]
    command: String,
}
fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let port = if let Ok(env_port) = std::env::var("DTN_WEB_PORT") {
        env_port // string is fine, no need to parse number
    } else {
        args.port.to_string()
    };
    let localhost = if args.ipv6 { "[::1]" } else { "127.0.0.1" };

    let client = DtnClient::with_host_and_port(
        localhost.into(),
        port.parse::<u16>().expect("invalid port number"),
    );

    client.register_application_endpoint(&args.endpoint)?;
    let mut config = WebSocketConfig::default();
    config.max_message_size = Some(128 * 1024 * 1024); // 128 MiB
    config.max_frame_size = Some(128 * 1024 * 1024); // 128 MiB
    let mut wscon = client.ws_with_config(config)?;

    wscon.write_text("/bundle")?;
    let msg = wscon.read_text()?;
    if msg.starts_with("200 tx mode: bundle") {
        eprintln!("[*] {}", msg);
    } else {
        bail!("[!] Failed to set mode to `bundle`");
    }

    wscon.write_text(&format!("/subscribe {}", args.endpoint))?;
    let msg = wscon.read_text()?;
    if msg.starts_with("200 subscribed") {
        eprintln!("[*] {}", msg);
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
                    Bundle::try_from(bin.to_vec()).expect("Error decoding bundle from server");
                if bndl.is_administrative_record() {
                    eprintln!("[!] Handling of administrative records not yet implemented!");
                } else if let Some(data) = bndl.payload() {
                    if args.verbose {
                        eprintln!("[<] Received Bundle-Id: {}", bndl.id());
                    }
                    if args.print {
                        let now = humantime::format_rfc3339(std::time::SystemTime::now());
                        println!(
                            "[{}] {} → {}",
                            now,
                            bndl.primary.source,
                            String::from_utf8_lossy(data)
                        );
                    } else {
                        let data_file = write_temp_file(data, args.verbose)?;
                        if args.verbose {
                            eprintln!("[*] wrote tmp data file, now executing...");
                        }
                        execute_cmd(&args.command, data_file, &bndl, args.verbose)?;
                    }
                } else if args.verbose {
                    eprintln!("[!] Unexpected payload!");
                    break;
                }
            }
            Message::Ping(_) => {
                if args.verbose {
                    eprintln!("[<] Ping")
                }
            }
            Message::Pong(_) => {
                if args.verbose {
                    eprintln!("[<] Pong")
                }
            }
            Message::Close(_) => {
                if args.verbose {
                    eprintln!("[<] Close")
                }
                break;
            }
            Message::Frame(_) => {
                if args.verbose {
                    eprintln!("[!] Received raw frame, not supported!")
                }
            }
        }
    }
    Ok(())
}
