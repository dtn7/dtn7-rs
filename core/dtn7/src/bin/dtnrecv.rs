use bp7::*;
use clap::Parser;
use std::convert::TryFrom;
use std::fs;
use std::io::prelude::*;
use std::process;

/// A simple Bundle Protocol 7 Receive Utility for Delay Tolerant Networking
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

    /// Specify local endpoint, e.g. 'incoming', or a group endpoint 'dtn://helpers/~incoming'
    #[clap(short, long, required_unless_present_any = ["bid", "delete"], conflicts_with_all = ["bid", "delete"])]
    endpoint: Option<String>,

    /// Download any bundle by its ID
    #[clap(short, long, required_unless_present_any = ["endpoint", "delete"])]
    bid: Option<String>,

    /// Delete any bundle by its ID
    #[clap(short, long, required_unless_present_any = ["endpoint", "bid"], value_name = "BID")]
    delete: Option<String>,

    /// Write bundle payload to file instead of stdout
    #[clap(short, long)]
    outfile: Option<String>,

    /// Hex output of whole bundle
    #[clap(short = 'x', long, conflicts_with = "raw")]
    hex: bool,

    /// Output full bundle in raw bytes, not only payload
    #[clap(short, long)]
    raw: bool,
}

fn write_bytes(data: &[u8], possible_file: Option<String>, verbose: bool) {
    if let Some(outfile) = possible_file {
        if verbose {
            println!("Writing to {}", outfile);
        }
        fs::write(outfile, data).expect("Unable to write file");
        if verbose {
            println!("Wrote {} bytes", data.len());
        }
    } else {
        std::io::stdout()
            .write_all(data)
            .expect("Error writing binary.");
    }
}
fn main() {
    let args = Args::parse();

    let port = if let Ok(env_port) = std::env::var("DTN_WEB_PORT") {
        env_port // string is fine no need to parse number
    } else {
        args.port.to_string()
    };
    let localhost = if args.ipv6 { "[::1]" } else { "127.0.0.1" };
    let local_url = if let Some(endpoint) = args.endpoint {
        format!("http://{}:{}/endpoint?{}", localhost, port, endpoint)
    } else if args.delete.is_some() {
        format!(
            "http://{}:{}/delete?{}",
            localhost,
            port,
            args.delete.clone().unwrap()
        )
    } else {
        format!(
            "http://{}:{}/download?{}",
            localhost,
            port,
            args.bid.unwrap()
        )
    };
    let res = attohttpc::get(local_url)
        .send()
        .expect("error connecting to local dtnd");

    if res.status() != attohttpc::StatusCode::OK {
        if args.verbose {
            println!("Unexpected response from server! {:?}", res);
        }
        process::exit(23);
    }
    let buf: Vec<u8> = res.bytes().expect("No bundle bytes received");
    if args.delete.is_some() {
        println!("Deleted bundle {}", args.delete.unwrap());
        process::exit(0);
    } else if buf.len() > 50 {
        // TODO: very arbitrary number, should check return code
        if args.hex {
            println!("{}", hexify(&buf));
        } else if args.raw {
            write_bytes(&buf, args.outfile, args.verbose);
        } else {
            let bndl: Bundle = Bundle::try_from(buf).expect("Error decoding bundle");
            match bndl
                .extension_block_by_type(bp7::canonical::PAYLOAD_BLOCK)
                .expect("Payload block missing!")
                .data()
            {
                bp7::canonical::CanonicalData::Data(data) => {
                    write_bytes(data, args.outfile, args.verbose);
                }
                _ => {
                    panic!("No data in payload block!");
                }
            }
        }
    } else if args.verbose {
        println!("Nothing to fetch.");
        process::exit(23);
    }
}
