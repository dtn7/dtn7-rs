use bp7::bundle::*;
use bp7::*;
use clap::{crate_authors, crate_version, App, Arg};
use std::io;
use std::io::prelude::*;

fn get_local_node_id(localhost: &str, port: &str) -> String {
    attohttpc::get(&format!("http://{}:{}/status/nodeid", localhost, port))
        .send()
        .expect("error connecting to local dtnd")
        .text()
        .unwrap()
}

fn get_cts(localhost: &str, port: &str) -> CreationTimestamp {
    let response = attohttpc::get(&format!("http://{}:{}/cts", localhost, port))
        .send()
        .expect("error connecting to local dtnd")
        .text()
        .unwrap();
    serde_json::from_str(&response).unwrap()
}

fn main() {
    let matches = App::new("dtnsend")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple Bundle Protocol 7 Send Utility for Delay Tolerant Networking")
        .arg(
            Arg::with_name("sender")
                .short("s")
                .long("sender")
                .value_name("SENDER")
                .help("Sets sender name (e.g. 'dtn://node1')")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("receiver")
                .short("r")
                .long("receiver")
                .value_name("RECEIVER")
                .help("Receiver EID (e.g. 'dtn://node2/incoming')")
                .required(true)
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
            Arg::with_name("lifetime")
                .short("l")
                .long("lifetime")
                .value_name("SECONDS")
                .help("Bundle lifetime in seconds (default = 3600)")
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
            Arg::with_name("dryrun")
                .short("D")
                .long("dry-run")
                .help("Don't actually send packet, just dump the encoded one.")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("ipv6")
                .short("6")
                .long("ipv6")
                .help("Use IPv6")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("infile")
                .index(1)
                .help("File to send, if omitted data is read from stdin till EOF"),
        )
        .get_matches();
    let localhost = if matches.is_present("ipv6") {
        "[::1]"
    } else {
        "127.0.0.1"
    };
    let dryrun: bool = matches.is_present("dryrun");
    let verbose: bool = matches.is_present("verbose");
    let port = std::env::var("DTN_WEB_PORT").unwrap_or_else(|_| "3000".into());
    let port = matches.value_of("port").unwrap_or(&port); // string is fine no need to parse number
    let sender: EndpointID = matches
        .value_of("sender")
        .unwrap_or(&get_local_node_id(localhost, port))
        .into();
    let receiver: EndpointID = matches.value_of("receiver").unwrap().into();
    let lifetime: u64 = matches
        .value_of("lifetime")
        .unwrap_or("3600")
        .parse::<u64>()
        .unwrap();
    let cts = get_cts(localhost, port);
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
    bndl.primary.bundle_control_flags = BUNDLE_MUST_NOT_FRAGMENTED | BUNDLE_STATUS_REQUEST_DELIVERY;
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
