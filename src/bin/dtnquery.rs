use clap::{crate_authors, crate_version, App, SubCommand};
use reqwest;


fn main() {
    let matches = App::new("dtnquery")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple Bundle Protocol 7 Query Utility for Delay Tolerant Networking")
        .subcommand(SubCommand::with_name("eids").about("list registered endpoint IDs"))
        .subcommand(SubCommand::with_name("peers").about("list known peers"))
        .subcommand(SubCommand::with_name("bundles").about("list bundles in store"))
        .subcommand(SubCommand::with_name("info").about("General dtnd info"))
        .get_matches();

    if let Some(_matches) = matches.subcommand_matches("eids") {
        println!("Listing registered endpoint IDs:");
        let res = reqwest::get("http://127.0.0.1:3000/status/eids")
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("peers") {
        println!("Listing of known peers:");
        let res = reqwest::get("http://127.0.0.1:3000/status/peers")
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("bundles") {
        println!("Listing of bundles in store:");
        let res = reqwest::get("http://127.0.0.1:3000/status/bundles")
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("info") {
        println!("Daemon info:");
        let res = reqwest::get("http://127.0.0.1:3000/status/info")
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
}