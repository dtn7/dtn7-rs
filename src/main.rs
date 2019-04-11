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
                .help("Sets local node name (e.g. 'dtn://node1'")
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
        .arg(
            Arg::with_name("routing")
                .short("r")
                .long("routing")
                .value_name("ROUTING")
                .help(&format!(
                    "Set routing algorithm: {}",
                    dtn7::routing::routing_algorithms().join(", ")
                ))
                .takes_value(true),
        )
        .arg(
            Arg::with_name("debug")
                .short("d")
                .long("debug")
                .help("Set log level to debug")
                .takes_value(false),
        )
        .get_matches();

    let config = matches.value_of("config").unwrap_or("default.conf"); // TODO: add support for config files

    cfg.nodeid = matches
        .value_of("nodeid")
        .unwrap_or("dtn://node1")
        .to_string();
    bp7::EndpointID::from(cfg.nodeid.clone()); // validate node id

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
    if let Some(r) = matches.value_of("routing") {
        if dtn7::routing::routing_algorithms().contains(&r) {
            cfg.routing = r.into();
        }
    }
    if matches.is_present("endpoint") {
        if let Some(in_v) = matches.values_of("endpoint") {
            for in_endpoint in in_v {
                cfg.endpoints.push(in_endpoint.to_string());
            }
        }
    }
    if matches.is_present("debug") {
        std::env::set_var("RUST_LOG", "dtn7=debug,dtnd=debug");
    } else {
        std::env::set_var("RUST_LOG", "dtn7=info,dtnd=info");
    }
    pretty_env_logger::init_timed();

    info!("starting dtnd");
    start_dtnd(cfg);
}
