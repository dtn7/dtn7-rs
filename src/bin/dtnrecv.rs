use bp7::*;
use clap::{crate_authors, crate_version, App, Arg};
use reqwest;
use std::fs;
use std::io::prelude::*;
use std::process;

fn main() {
    let matches = App::new("dtnrecv")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple Bundle Protocol 7 Receive Utility for Delay Tolerant Networking")
        .arg(
            Arg::with_name("endpoint")
                .short("e")
                .long("endpoint")
                .value_name("ENDPOINT")
                .help("Specify local endpoint, e.g. '/incoming')")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("outfile")
                .short("o")
                .long("output")
                .value_name("FILE")
                .help("Write bundle payload to file instead of stdout")
                .required(false)
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
        .get_matches();

    let verbose: bool = matches.is_present("verbose");
    let endpoint: String = matches.value_of("endpoint").unwrap().into();
    let port = std::env::var("DTN_WEB_PORT").unwrap_or_else(|_| "3000".into());
    let port = matches.value_of("port").unwrap_or(&port); // string is fine no need to parse number

    let local_url = format!("http://127.0.0.1:{}/endpoint?{}", port, endpoint);
    let mut res = reqwest::get(&local_url).expect("error connecting to local dtnd");

    if res.content_length() > Some(10) {
        let mut buf: Vec<u8> = vec![];
        res.copy_to(&mut buf).unwrap();

        let mut bndl: Bundle = Bundle::from(buf);
        match bndl
            .extension_block(bp7::canonical::PAYLOAD_BLOCK)
            .expect("Payload block missing!")
            .get_data()
        {
            bp7::canonical::CanonicalData::Data(data) => {
                if let Some(outfile) = matches.value_of("outfile") {
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
                        .expect("Error sending binary to server.");
                }
            }
            _ => {
                panic!("No data in payload block!");
            }
        }
    } else {
        if verbose {
            println!("Nothing to fetch.");
            process::exit(23);
        }
    }
}
