use clap::{crate_authors, crate_version, App, Arg, SubCommand};

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
        .subcommand(SubCommand::with_name("bundles").about("list bundles in node"))
        .subcommand(SubCommand::with_name("store").about("list bundles status in store"))
        .subcommand(SubCommand::with_name("info").about("General dtnd info"))
        .subcommand(SubCommand::with_name("nodeid").about("Local node id"))
        .get_matches();
    let port = std::env::var("DTN_WEB_PORT").unwrap_or_else(|_| "3000".into());
    let port = matches.value_of("port").unwrap_or(&port); // string is fine no need to parse number

    if let Some(_matches) = matches.subcommand_matches("nodeid") {
        println!("Local node ID:");
        let res = attohttpc::get(&format!("http://127.0.0.1:{}/status/nodeid", port))
            .send()
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("eids") {
        println!("Listing registered endpoint IDs:");
        let res = attohttpc::get(&format!("http://127.0.0.1:{}/status/eids", port))
            .send()
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("peers") {
        println!("Listing of known peers:");
        let res = attohttpc::get(&format!("http://127.0.0.1:{}/status/peers", port))
            .send()
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("bundles") {
        println!("Listing of bundles in store:");
        let res = attohttpc::get(&format!("http://127.0.0.1:{}/status/bundles", port))
            .send()
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("store") {
        println!("Listing of bundles status in store:");
        let res = attohttpc::get(&format!("http://127.0.0.1:{}/status/store", port))
            .send()
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("info") {
        println!("Daemon info:");
        let res = attohttpc::get(&format!("http://127.0.0.1:{}/status/info", port))
            .send()
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
}
