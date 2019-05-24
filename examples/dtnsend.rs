use bp7::*;
use clap::{crate_authors, crate_version, App, Arg};
use reqwest;
use std::io;
use std::io::prelude::*;


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
                .help("Sets sender name (e.g. 'dtn://node1/dtnsend')")
                .required(true)
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
            Arg::with_name("infile")
                .index(1)
                .help("File to send, if omitted data is read from stdin till EOF"),
        )
        .get_matches();

    let sender: EndpointID = matches.value_of("sender").unwrap().into();
    let receiver: EndpointID = matches.value_of("receiver").unwrap().into();

    let mut buffer = Vec::new();
    if let Some(infile) = matches.value_of("infile") {
        println!("Sending {}", infile);
        let mut f = std::fs::File::open(infile).expect("Error accessing file.");
        f.read_to_end(&mut buffer)
            .expect("Error reading from file.");
    } else {
        io::stdin()
            .read_to_end(&mut buffer)
            .expect("Error reading from stdin.");
    }

    println!("Sending {} bytes.", buffer.len());

    let bndl = bp7::bundle::new_std_payload_bundle(sender, receiver, buffer).to_cbor();
    let hexstr = bp7::helpers::hexify(&bndl);
    println!("{}", hexstr);

    //let local_url = format!("http://127.0.0.1:3000/send?{}", hexstr);
    //let res = reqwest::get(&local_url).expect("error connecting to local dtnd").text().unwrap();
    let client = reqwest::Client::new();
    let res = client
        .post("http://127.0.0.1:3000/send")
        .body(hexstr)
        .send()
        .expect("error send bundle to dtnd")
        .text()
        .unwrap();

    //let res = reqwest::get(&local_url).expect("error connecting to local dtnd").text().unwrap();

    println!("Result {:?}", res);
}