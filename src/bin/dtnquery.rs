use clap::{crate_authors, crate_version, App, Arg, SubCommand};
use reqwest;

fn main() {
    let matches = App::new("dtnquery")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple Bundle Protocol 7 Query Utility for Delay Tolerant Networking")
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .value_name("PORT")
                .help("Local web port (default = 3000)")
                .required(false)
                .takes_value(true),
        )
        .subcommand(SubCommand::with_name("eids").about("list registered endpoint IDs"))
        .subcommand(SubCommand::with_name("peers").about("list known peers"))
        .subcommand(SubCommand::with_name("bundles").about("list bundles in store"))
        .subcommand(SubCommand::with_name("info").about("General dtnd info"))
        .get_matches();
    let port = std::env::var("DTN_WEB_PORT").unwrap_or_else(|_| "3000".into());
    let port = matches.value_of("port").unwrap_or(&port); // string is fine no need to parse number

    if let Some(_matches) = matches.subcommand_matches("eids") {
        println!("Listing registered endpoint IDs:");
        let res = reqwest::get(&format!("http://127.0.0.1:{}/status/eids", port))
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("peers") {
        println!("Listing of known peers:");
        let res = reqwest::get(&format!("http://127.0.0.1:{}/status/peers", port))
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("bundles") {
        println!("Listing of bundles in store:");
        let res = reqwest::get(&format!("http://127.0.0.1:{}/status/bundles", port))
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("info") {
        println!("Daemon info:");
        let res = reqwest::get(&format!("http://127.0.0.1:{}/status/info", port))
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
}
