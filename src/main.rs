use clap::{App, Arg, ArgGroup, SubCommand};
use dtn7::dtnd::daemon::*;
use dtn7::DtnConfig;
use log::{info, trace, warn};
use pretty_env_logger;

fn main() {
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

    let mut cfg = DtnConfig::new();

    let matches = App::new("dtn7-rs")
        .version(VERSION)
        .author(AUTHORS)
        .about("A simple Bundle Protocol 7 Daemon for Delay Tolerant Networking")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("Sets a custom config file")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("nodeid")
                .short("n")
                .long("nodeid")
                .value_name("NODEID")
                .help("Sets local node name")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("endpoint")
                .short("e")
                .long("endpoint")
                .value_name("ENDPOINT")
                .help("Registers an application agent for an endpoint")
                .multiple(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("interval")
                .short("i")
                .long("interval")
                .value_name("INTERVAL")
                .help("Sets service discovery interval")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("janitor")
                .short("j")
                .long("janitor")
                .value_name("INTERVAL")
                .help("Sets janitor interval")
                .takes_value(true),
        )
        .get_matches();

    let config = matches.value_of("config").unwrap_or("default.conf");
    println!("Value for config: {}", config);

    cfg.nodeid = matches.value_of("nodeid").unwrap_or("node1").to_string();
    println!("Value for nodeid: {}", cfg.nodeid);

    if let Some(i) = matches.value_of("interval") {
        cfg.announcement_interval = i
            .parse::<u64>()
            .expect("Could not parse interval parameter!");
    }

    if let Some(i) = matches.value_of("janitor") {
        cfg.janitor_interval = i
            .parse::<u64>()
            .expect("Could not parse janitor parameter!");
    }
    if matches.is_present("endpoint") {
        if let Some(in_v) = matches.values_of("endpoint") {
            for in_endpoint in in_v {
                println!("An endpoint: {}", in_endpoint);
                cfg.endpoints.push(in_endpoint.to_string());
            }
        }
    }
    std::env::set_var("RUST_LOG", "dtn7=debug,dtnd=debug");
    pretty_env_logger::init_timed();

    info!("starting dtnd");
    start_dtnd(cfg);
}
