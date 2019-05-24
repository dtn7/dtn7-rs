use bp7::*;
use clap::{crate_authors, crate_version, App, Arg};
use reqwest;
use std::io;
use std::io::prelude::*;
use std::fs;


fn main() {
    let matches = App::new("dtnrecv")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple Bundle Protocol 7 Send Utility for Delay Tolerant Networking")
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
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .help("verbose output")
                .takes_value(false),
        )
        
        .get_matches();

    let verbose: bool = matches.is_present("verbose");
    let endpoint: String = matches.value_of("endpoint").unwrap().into();

    let local_url = format!("http://127.0.0.1:3000/endpoint?{}", endpoint);
    let res = reqwest::get(&local_url).expect("error connecting to local dtnd").text().unwrap();
    if res.len() > 10 {
        let mut bndl : Bundle = Bundle::from(bp7::helpers::unhexify(&res).unwrap());
        match bndl.extension_block(bp7::canonical::PAYLOAD_BLOCK).expect("Payload block missing!").get_data() {
            bp7::canonical::CanonicalData::Data(data) => {
                //println!("{}", String::from_utf8(data.to_vec()).unwrap());
                if let Some(outfile) = matches.value_of("outfile") {
                    if verbose {
                        println!("Writing to {}", outfile);
                    }
                    fs::write(outfile, data).expect("Unable to write file");
                    if verbose {
                        println!("Wrote {} bytes", data.len());
                    }
                } else {
                    std::io::stdout().write_all(data);
                }
            }
            _ => {
                panic!("No data in payload block!");
            }
         }
    } else {
        //dbg!(&res);
        if verbose {
            println!("Nothing to fetch.");
        }
    }
}