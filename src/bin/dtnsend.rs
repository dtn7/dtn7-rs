use bp7::*;
use chrono::{DateTime, Utc};
use clap::{crate_authors, crate_version, App, Arg};
use reqwest;
use std::io;
use std::io::prelude::*;

fn get_local_node_id(port: &str) -> String {
    reqwest::get(&format!("http://127.0.0.1:{}/status/nodeid", port))
        .expect("error connecting to local dtnd")
        .text()
        .unwrap()
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
            Arg::with_name("infile")
                .index(1)
                .help("File to send, if omitted data is read from stdin till EOF"),
        )
        .get_matches();

    let dryrun: bool = matches.is_present("dryrun");
    let verbose: bool = matches.is_present("verbose");
    let port = std::env::var("DTN_WEB_PORT").unwrap_or_else(|_| "3000".into());
    let port = matches.value_of("port").unwrap_or(&port); // string is fine no need to parse number
    let sender: EndpointID = matches
        .value_of("sender")
        .unwrap_or(&get_local_node_id(port))
        .into();
    let receiver: EndpointID = matches.value_of("receiver").unwrap().into();

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

    let mut bndl = bp7::bundle::new_std_payload_bundle(sender, receiver, buffer);
    let binbundle = bndl.to_cbor();
    println!("Bundle-Id: {}", bndl.id());
    if verbose || dryrun {
        let hexstr = bp7::helpers::hexify(&binbundle);
        println!("{}", hexstr);
    }

    //let local_url = format!("http://127.0.0.1:3000/send?{}", hexstr);
    //let res = reqwest::get(&local_url).expect("error connecting to local dtnd").text().unwrap();
    if !dryrun {
        let client = reqwest::Client::new();
        let res = client
            .post(&format!("http://127.0.0.1:{}/send", port))
            .body(binbundle)
            .send()
            .expect("error send bundle to dtnd")
            .text()
            .unwrap();
        println!("Result: {}", res);
        let now: DateTime<Utc> = Utc::now();
        println!("Time: {}", now);
    }
}
