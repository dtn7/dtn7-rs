use clap::{crate_authors, crate_version, App, Arg};

fn main() {
    let matches = App::new("dtnquery")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple Bundle Protocol 7 Query Utility for Delay Tolerant Networking")
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
            Arg::new("ipv6")
                .short('6')
                .long("ipv6")
                .help("Use IPv6")
                .takes_value(false),
        )
        .subcommand(App::new("eids").about("list registered endpoint IDs"))
        .subcommand(App::new("peers").about("list known peers"))
        .subcommand(
            App::new("bundles").about("list bundles in node").arg(
                Arg::new("v")
                    .short('v')
                    .multiple_occurrences(false)
                    .help("Verbose output includes bundle destination"),
            ),
        )
        .subcommand(App::new("store").about("list bundles status in store"))
        .subcommand(App::new("info").about("General dtnd info"))
        .subcommand(App::new("nodeid").about("Local node id"))
        .get_matches();
    let port = std::env::var("DTN_WEB_PORT").unwrap_or_else(|_| "3000".into());
    let port = matches.value_of("port").unwrap_or(&port); // string is fine no need to parse number

    let localhost = if matches.is_present("ipv6") {
        "[::1]"
    } else {
        "127.0.0.1"
    };
    if let Some(_matches) = matches.subcommand_matches("nodeid") {
        println!("Local node ID:");
        let res = attohttpc::get(&format!("http://{}:{}/status/nodeid", localhost, port))
            .send()
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("eids") {
        println!("Listing registered endpoint IDs:");
        let res = attohttpc::get(&format!("http://{}:{}/status/eids", localhost, port))
            .send()
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("peers") {
        println!("Listing of known peers:");
        let res = attohttpc::get(&format!("http://{}:{}/status/peers", localhost, port))
            .send()
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(matches) = matches.subcommand_matches("bundles") {
        let verbose = matches.occurrences_of("v");
        println!("Listing of bundles in store:");
        let query_url = if verbose == 0 {
            format!("http://{}:{}/status/bundles", localhost, port)
        } else {
            format!("http://{}:{}/status/bundles_dest", localhost, port)
        };
        let res = attohttpc::get(&query_url)
            .send()
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("store") {
        println!("Listing of bundles status in store:");
        let res = attohttpc::get(&format!("http://{}:{}/status/store", localhost, port))
            .send()
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
    if let Some(_matches) = matches.subcommand_matches("info") {
        println!("Daemon info:");
        let res = attohttpc::get(&format!("http://{}:{}/status/info", localhost, port))
            .send()
            .expect("error connecting to local dtnd")
            .text()
            .unwrap();
        println!("{}", res);
    }
}
