use clap::{crate_authors, crate_version, App, Arg};
use dtn7::dtnd::daemon::*;
use dtn7::DtnConfig;
use log::info;
use pretty_env_logger;
use std::panic;
use std::process;

fn main() {
    let mut cfg = DtnConfig::new();

    if cfg!(debug_assertions) {
        // Whenever a threads has a panic, quit the whole program!
        panic::set_hook(Box::new(|p| {
            println!("Panic hook: {}", p);
            process::exit(1);
        }));
    }

    let matches = App::new("dtn7-rs")
        .version(crate_version!())
        .author(crate_authors!())
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
                .help("Sets local node name (e.g. 'dtn://node1')")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("endpoint")
                .short("e")
                .long("endpoint")
                .value_name("ENDPOINT")
                .help("Registers an application agent for a node local endpoint (e.g. 'incoming' listens on 'dtn://node1/incoming')")
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
            Arg::with_name("peertimeout")
                .short("p")
                .long("peer-timeout")
                .value_name("SECONDS")
                .help("Sets timeout to remove peer")
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
            Arg::with_name("cla")
                .short("C")
                .long("cla")
                .value_name("CLA")
                .help(&format!(
                    "Add convergency layer agent: {}",
                    dtn7::cla::convergency_layer_agents().join(", ")
                ))
                .multiple(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("staticpeer")
                .short("s")
                .long("static-peer")
                .value_name("PEER")
                .help("Adds a static peer (e.g. stcp://192.168.2.1/node2)")
                .multiple(true)
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

    if matches.is_present("debug") || cfg.debug {
        std::env::set_var("RUST_LOG", "dtn7=debug,dtnd=debug");
    } else {
        std::env::set_var("RUST_LOG", "dtn7=info,dtnd=info");
    }
    pretty_env_logger::init_timed();

    if let Some(cfgfile) = matches.value_of("config") {
        cfg = DtnConfig::from(std::path::PathBuf::from(cfgfile));
    }
    //let _config = matches.value_of("config").unwrap_or("default.conf"); // TODO: add support for config files

    if let Some(nodeid) = matches.value_of("nodeid") {
        cfg.nodeid = nodeid.to_string();
    }
    bp7::EndpointID::from(format!("dtn://{}", cfg.nodeid.clone())); // validate node id

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

    if let Some(t) = matches.value_of("peertimeout") {
        cfg.peer_timeout = t
            .parse::<u64>()
            .expect("Could not parse peer timeout parameter!");
    }

    if let Some(r) = matches.value_of("routing") {
        if dtn7::routing::routing_algorithms().contains(&r) {
            cfg.routing = r.into();
        }
    }

    if let Some(clas) = matches.values_of("cla") {
        for cla in clas {
            if dtn7::cla::convergency_layer_agents().contains(&cla) {
                cfg.clas.push(cla.to_string());
            }
        }
    }
    if let Some(statics) = matches.values_of("staticpeer") {
        for s in statics {
            cfg.statics
                .push(dbg!(dtn7::core::helpers::parse_peer_url(s)));
        }
    }

    if let Some(in_v) = matches.values_of("endpoint") {
        for in_endpoint in in_v {
            cfg.endpoints.push(in_endpoint.to_string());
        }
    }

    // historic code, still neccessary?!
    // Load config second time for logging purposes
    //if let Some(cfgfile) = matches.value_of("config") {
    //DtnConfig::from(std::path::PathBuf::from(cfgfile));
    //}
    info!("starting dtnd");
    start_dtnd(cfg);
}
