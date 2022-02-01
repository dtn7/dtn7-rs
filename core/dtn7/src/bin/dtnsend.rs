use bp7::bundle::*;
use bp7::flags::{BundleControlFlags, BundleValidation};
use bp7::*;
use clap::{crate_authors, crate_version, App, Arg};
use dtn7_plus::client::DtnClient;
use std::io;
use std::{convert::TryInto, io::prelude::*};

fn main() {
    let matches = App::new("dtnsend")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple Bundle Protocol 7 Send Utility for Delay Tolerant Networking")
        .arg(
            Arg::new("sender")
                .short('s')
                .long("sender")
                .value_name("SENDER")
                .help("Sets sender name (e.g. 'dtn://node1')")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::new("receiver")
                .short('r')
                .long("receiver")
                .value_name("RECEIVER")
                .help("Receiver EID (e.g. 'dtn://node2/incoming')")
                .required(true)
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
            Arg::new("lifetime")
                .short('l')
                .long("lifetime")
                .value_name("SECONDS")
                .help("Bundle lifetime in seconds (default = 3600)")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("verbose output")
                .takes_value(false),
        )
        .arg(
            Arg::new("dryrun")
                .short('D')
                .long("dry-run")
                .help("Don't actually send packet, just dump the encoded one.")
                .takes_value(false),
        )
        .arg(
            Arg::new("ipv6")
                .short('6')
                .long("ipv6")
                .help("Use IPv6")
                .takes_value(false),
        )
        .arg(
            Arg::new("infile")
                .index(1)
                .help("File to send, if omitted data is read from stdin till EOF"),
        )
        .get_matches();
    let localhost = if matches.is_present("ipv6") {
        "[::1]"
    } else {
        "127.0.0.1"
    };
    let port = std::env::var("DTN_WEB_PORT").unwrap_or_else(|_| "3000".into());
    let port = matches.value_of("port").unwrap_or(&port); // string is fine no need to parse number
    let client = DtnClient::with_host_and_port(
        localhost.into(),
        port.parse::<u16>().expect("invalid port number"),
    );
    let dryrun: bool = matches.is_present("dryrun");
    let verbose: bool = matches.is_present("verbose");
    let sender: EndpointID = matches
        .value_of("sender")
        .unwrap_or(
            &client
                .local_node_id()
                .expect("error getting node id from local dtnd")
                .to_string(),
        )
        .try_into()
        .unwrap();
    let receiver: EndpointID = matches.value_of("receiver").unwrap().try_into().unwrap();
    let lifetime: u64 = matches
        .value_of("lifetime")
        .unwrap_or("3600")
        .parse::<u64>()
        .unwrap();
    let cts = client
        .creation_timestamp()
        .expect("error getting creation timestamp from local dtnd");
    let mut buffer = Vec::new();
    if let Some(infile) = matches.value_of("infile") {
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
