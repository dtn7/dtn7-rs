use bp7::bundle::*;
use bp7::flags::{BundleControlFlags, BundleValidation};
use bp7::*;
use dtn7_plus::client::DtnClient;
use gumdrop::Options;
use std::{convert::TryInto, io::prelude::*};
use std::{io, process};

/// A simple Bundle Protocol 7 Send Utility for Delay Tolerant Networking
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
    /// Sets sender name (e.g. 'dtn://node1')'
    #[options(short = "s", long = "sender")]
    sender: Option<String>,
    /// Receiver EID (e.g. 'dtn://node2/incoming')
    #[options(short = "r", long = "receiver", required)]
    receiver: String,
    /// Bundle lifetime in seconds
    #[options(short = "l", long = "lifetime", default = "3600")]
    lifetime: u64,
    /// File to send, if omitted data is read from stdin till EOF
    #[options(short = "i", long = "infile")]
    infile: Option<String>,
    /// Don't actually send packet, just dump the encoded one.
    #[options(short = "d", long = "dry-run")]
    dryrun: bool,
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

    let verbose: bool = opts.verbose;
    let port = std::env::var("DTN_WEB_PORT").unwrap_or_else(|_| opts.port.to_string());

    let localhost = if opts.ipv6 { "[::1]" } else { "127.0.0.1" };

    let client = DtnClient::with_host_and_port(
        localhost.into(),
        port.parse::<u16>().expect("invalid port number"),
    );
    let dryrun: bool = opts.dryrun;
    let sender: EndpointID = opts
        .sender
        .unwrap_or_else(|| {
            client
                .local_node_id()
                .expect("error getting node id from local dtnd")
                .to_string()
        })
        .try_into()
        .unwrap();
    let receiver: EndpointID = opts.receiver.try_into().unwrap();
    let lifetime: u64 = opts.lifetime;
    let cts = client
        .creation_timestamp()
        .expect("error getting creation timestamp from local dtnd");
    let mut buffer = Vec::new();
    if let Some(infile) = opts.infile {
        if verbose {
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

    if verbose {
        println!("Sending {} bytes.", buffer.len());
    }

    let mut bndl = new_std_payload_bundle(sender, receiver, buffer);
    let flags = BundleControlFlags::BUNDLE_MUST_NOT_FRAGMENTED
        | BundleControlFlags::BUNDLE_STATUS_REQUEST_DELIVERY;
    bndl.primary.bundle_control_flags.set(flags);
    bndl.primary.creation_timestamp = cts;
    bndl.primary.lifetime = std::time::Duration::from_secs(lifetime);
    let binbundle = bndl.to_cbor();
    println!("Bundle-Id: {}", bndl.id());
    if verbose || dryrun {
        let hexstr = bp7::helpers::hexify(&binbundle);
        println!("{}", hexstr);
    }

    //let local_url = format!("http://127.0.0.1:3000/send?bundle={}", hexstr);
    //let res = reqwest::get(&local_url).expect("error connecting to local dtnd").text().unwrap();
    if !dryrun {
        let res = attohttpc::post(&format!("http://{}:{}/insert", localhost, port))
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
