use bp7::*;
use gumdrop::Options;
use std::convert::TryFrom;
use std::fs;
use std::io::prelude::*;
use std::process;

/// A simple Bundle Protocol 7 Receive Utility for Delay Tolerant Networking
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
    /// Specify local endpoint, e.g. '/incoming', or a group endpoint 'dtn://helpers/~incoming'
    #[options(short = "e", long = "endpoint")]
    endpoint: Option<String>,
    /// Local web port (default = 3000)
    #[options(short = "p", long = "port", default = "3000")]
    port: u16,
    /// hex output of whole bundle
    #[options(short = "x", long = "hex")]
    hex: bool,
    /// output full bundle in raw bytes, not only payload
    #[options(short = "r", long = "raw")]
    raw: bool,
    /// Write bundle payload to file instead of stdout
    #[options(short = "o", long = "outfile")]
    outfile: Option<String>,
    /// Download any bundle by ID
    #[options(short = "b", long = "bid")]
    bid: Option<String>,
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
    let opts = CmdOptions::parse_args_default_or_exit();

    if opts.help {
        println!("{}", CmdOptions::usage());
        process::exit(0);
    }
    if opts.version {
        println!("{}", dtn7::VERSION);
        process::exit(0);
    }
    if opts.endpoint.is_none() && opts.bid.is_none() {
        eprintln!("You must specify either an endpoint or a bundle ID");
        println!("{}", CmdOptions::usage());
        process::exit(1);
    }

    let verbose: bool = opts.verbose;
    let hex: bool = opts.hex;
    let raw: bool = opts.raw;
    let port = std::env::var("DTN_WEB_PORT").unwrap_or_else(|_| opts.port.to_string());

    let localhost = if opts.ipv6 { "[::1]" } else { "127.0.0.1" };
    let local_url = if let Some(endpoint) = opts.endpoint {
        format!("http://{}:{}/endpoint?{}", localhost, port, endpoint)
    } else {
        format!(
            "http://{}:{}/download?{}",
            localhost,
            port,
            opts.bid.unwrap()
        )
    };
    let res = attohttpc::get(&local_url)
        .send()
        .expect("error connecting to local dtnd");

    if res.status() != attohttpc::StatusCode::OK {
        if verbose {
            println!("Unexpected response from server! {:?}", res);
        }
        process::exit(23);
    }
    let buf: Vec<u8> = res.bytes().expect("No bundle bytes received");
    if buf.len() > 50 {
        // TODO: very arbitrary number, should check return code
        if hex {
            println!("{}", hexify(&buf));
        } else if raw {
            write_bytes(&buf, opts.outfile, verbose);
        } else {
            let bndl: Bundle = Bundle::try_from(buf).expect("Error decoding bundle");
            match bndl
                .extension_block_by_type(bp7::canonical::PAYLOAD_BLOCK)
                .expect("Payload block missing!")
                .data()
            {
                bp7::canonical::CanonicalData::Data(data) => {
                    write_bytes(data, opts.outfile, verbose);
                }
                _ => {
                    panic!("No data in payload block!");
                }
            }
        }
    } else if verbose {
        println!("Nothing to fetch.");
        process::exit(23);
    }
}
