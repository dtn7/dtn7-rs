use bp7::bundle::*;
use bp7::flags::{BundleControlFlags, BundleValidation};
use bp7::*;
use clap::Parser;
use dtn7_plus::client::DtnClient;
use std::io;
use std::{convert::TryInto, io::prelude::*};

/// A simple Bundle Protocol 7 Send Utility for Delay Tolerant Networking
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

    /// Sets sender name (e.g. 'dtn://node1')
    #[clap(short, long)]
    sender: Option<String>,

    /// Receiver EID (e.g. 'dtn://node2/incoming')
    #[clap(short, long)]
    receiver: String,

    /// File to send, if omitted, data is read from stdin till EOF
    #[clap(index = 1)]
    infile: Option<String>,

    /// Don't actually send packet, just dump the encoded one.
    #[clap(short = 'D', long)]
    dryrun: bool,

    /// Bundle lifetime in seconds (default = 3600)
    #[clap(short, long, default_value_t = 3600)]
    lifetime: u64,
}

fn main() {
    let args = Args::parse();
    let localhost = if args.ipv6 { "[::1]" } else { "127.0.0.1" };
    let port = if let Ok(env_port) = std::env::var("DTN_WEB_PORT") {
        env_port // string is fine, no need to parse number
    } else {
        args.port.to_string()
    };
    let client = DtnClient::with_host_and_port(
        localhost.into(),
        port.parse::<u16>().expect("invalid port number"),
    );
    let sender: EndpointID = args
        .sender
        .unwrap_or_else(|| {
            client
                .local_node_id()
                .expect("error getting node id from local dtnd")
                .to_string()
        })
        .try_into()
        .unwrap();
    let receiver: EndpointID = args.receiver.try_into().unwrap();
    let cts = client
        .creation_timestamp()
        .expect("error getting creation timestamp from local dtnd");
    let mut buffer = Vec::new();
    if let Some(infile) = args.infile {
        if args.verbose {
            println!("Sending {}", infile);
        }
        let mut f = std::fs::File::open(infile).expect("Error accessing file.");
        f.read_to_end(&mut buffer)
            .expect("Error reading from file.");
    } else {
        io::stdin()
            .read_to_end(&mut buffer)
            .expect("Error reading from stdin.");
    }

    if args.verbose {
        println!("Sending {} bytes.", buffer.len());
    }

    let mut bndl = new_std_payload_bundle(sender, receiver, buffer);
    let flags = BundleControlFlags::BUNDLE_MUST_NOT_FRAGMENTED
        | BundleControlFlags::BUNDLE_STATUS_REQUEST_DELIVERY;
    bndl.primary.bundle_control_flags.set(flags);
    bndl.primary.creation_timestamp = cts;
    bndl.primary.lifetime = std::time::Duration::from_secs(args.lifetime);
    let binbundle = bndl.to_cbor();
    println!("Bundle-Id: {}", bndl.id());
    if args.verbose || args.dryrun {
        let hexstr = bp7::helpers::hexify(&binbundle);
        println!("{}", hexstr);
    }

    //let local_url = format!("http://127.0.0.1:3000/send?bundle={}", hexstr);
    //let res = reqwest::get(&local_url).expect("error connecting to local dtnd").text().unwrap();
    if !args.dryrun {
        let res = attohttpc::post(format!("http://{}:{}/insert", localhost, port))
            .bytes(binbundle)
            .send()
            .expect("error send bundle to dtnd")
            .text()
            .unwrap();
        println!("Result: {}", res);
        let now = std::time::SystemTime::now();
        println!("Time: {}", humantime::format_rfc3339(now));
    }
}
