#![recursion_limit = "256"]

use clap::{crate_authors, crate_version, App, Arg};
use dtn7::dtnd::daemon::*;
use dtn7::DtnConfig;
use log::info;
use std::panic;
use std::{convert::TryInto, process};

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
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
            Arg::with_name("workdir")
                .short("W")
                .long("workdir")
                .value_name("PATH")
                .help("Sets the working directory (e.g. '/tmp/node1', default '.')")
                .takes_value(true),
        ).arg(
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
                .value_name("humantime")
                .help("Sets service discovery interval (0 = deactive, 2s = 2 seconds, 3m = 3 minutes, etc.) Refers to the discovery interval that is advertised when flag -b is set")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("janitor")
                .short("j")
                .long("janitor")
                .value_name("humantime")
                .help("Sets janitor interval (0 = deactive, 2s = 2 seconds, 3m = 3 minutes, etc.)")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("discoverydestination")
                .short("E")
                .long("discovery-destination")
                .value_name("DD[:port]")
                .help("Sets destination beacons shall be sent to for discovery purposes (default IPv4 = 224.0.0.26:3003, IPv6 = [FF02::300]:3003")
                .multiple(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("webport")
                .short("w")
                .long("web-port")
                .value_name("PORT")
                .help("Sets web interface port (default = 3000)")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("peertimeout")
                .short("p")
                .long("peer-timeout")
                .value_name("humantime")
                .help("Sets timeout to remove peer (default = 20s)")
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
        ).arg(
            Arg::with_name("db")
                .short("D")
                .long("db")
                .value_name("STORE")
                .help(&format!(
                    "Set bundle store: {}",
                    dtn7::core::store::bundle_stores().join(", ")
                ))
                .takes_value(true),
        )
        .arg(
            Arg::with_name("cla")
                .short("C")
                .long("cla")
                .value_name("CLA[:local_port]")
                .help(&format!(
                    "Add convergence layer agent: {}",
                    dtn7::cla::convergence_layer_agents().join(", ")
                ))
                .multiple(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("service")
                .short("S")
                .long("service")
                .value_name("TAG:payload")
                .help("Add a self defined service.")
                .long_help("Tag 63 can be used for any kind of unformatted string message. Usage: -S 63:'Hello World'
Tag 127 takes 2 floats and is interpreted as latitude/longitude. Usage: -S 127:'52.32 24.42'
Tag 191 takes 1 integer and is interpreted as battery level in %. Usage: -S 191:71
Tag 255 takes 5 arguments and is interpreted as address. Usage: -S 255:'Samplestreet 42 12345 SampleCity SC'")
                .multiple(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("staticpeer")
                .short("s")
                .long("static-peer")
                .value_name("PEER")
                .help("Adds a static peer (e.g. mtcp://192.168.2.1:2342/node2)")
                .multiple(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("beacon-period")
                .short("b")
                .long("beacon-period")
                .help("Enables the advertisement of the beacon sending interval to inform neighbors about when to expect new beacons")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("debug")
                .short("d")
                .long("debug")
                .help("Set log level to debug")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("generate-status-reports")
                .short("g")
                .long("generate-status-reports")
                .help("Generate status report bundles, can lead to a lot of traffic (default: deactivated)")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("unsafe_httpd")
                .short("U")
                .long("unsafe-httpd")
                .help("Allow httpd RPC calls from anyhwere")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("ipv4")
                .short("4")
                .long("ipv4")
                .help("Use IPv4")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("ipv6")
                .short("6")
                .long("ipv6")
                .help("Use IPv6")
                .takes_value(false),
        )
        .get_matches();

    if matches.is_present("debug") || cfg.debug {
        std::env::set_var(
            "RUST_LOG",
            "dtn7=debug,dtnd=debug,actix_server=debug,actix_web=debug",
        );
    } else {
        std::env::set_var(
            "RUST_LOG",
            "dtn7=info,dtnd=info,actix_server=info,actix_web=info",
        );
    }
    pretty_env_logger::init_timed();

    if matches.is_present("ipv6") {
        cfg.v6 = true;
        cfg.v4 = false;
    }
    cfg.v4 = matches.is_present("ipv4") || cfg.v4;
    cfg.generate_status_reports =
        matches.is_present("generate-status-reports") || cfg.generate_status_reports;

    cfg.unsafe_httpd = matches.is_present("unsafe_httpd") || cfg.unsafe_httpd;
    cfg.enable_period = matches.is_present("beacon-period");
    if let Some(cfgfile) = matches.value_of("config") {
        cfg = DtnConfig::from(std::path::PathBuf::from(cfgfile));
    }

    if let Some(workdir) = matches.value_of("workdir") {
        cfg.workdir = std::path::PathBuf::from(workdir);
    }

    if let Some(nodeid) = matches.value_of("nodeid") {
        if nodeid.chars().all(char::is_alphanumeric) {
            cfg.host_eid = if let Ok(number) = nodeid.parse::<u64>() {
                format!("ipn:{}.0", number).try_into().unwrap()
            } else {
                format!("dtn://{}", nodeid).try_into().unwrap()
            };
        } else {
            cfg.host_eid = nodeid.try_into().unwrap();
            if !cfg.host_eid.is_node_id() {
                panic!("Invalid node id!");
            }
        }
    }

    if let Some(i) = matches.value_of("interval") {
        if i == "0" {
            cfg.announcement_interval = std::time::Duration::new(0, 0);
        } else {
            cfg.announcement_interval =
                humantime::parse_duration(i).expect("Could not parse interval parameter!");
        }
    }
    if let Some(i) = matches.value_of("webport") {
        cfg.webport = i
            .parse::<u16>()
            .expect("Could not parse web port parameter!");
    }

    if let Some(i) = matches.value_of("janitor") {
        if i == "0" {
            cfg.janitor_interval = std::time::Duration::new(0, 0);
        } else {
            cfg.janitor_interval =
                humantime::parse_duration(i).expect("Could not parse janitor parameter!");
        }
    }
    if let Some(i) = matches.value_of("peertimeout") {
        if i == "0" {
            cfg.peer_timeout = std::time::Duration::new(0, 0);
        } else {
            cfg.peer_timeout =
                humantime::parse_duration(i).expect("Could not parse peer timeout parameter!");
        }
    }

    if let Some(r) = matches.value_of("routing") {
        if dtn7::routing::routing_algorithms().contains(&r) {
            cfg.routing = r.into();
        }
    }

    if let Some(db) = matches.value_of("db") {
        if dtn7::core::store::bundle_stores().contains(&db) {
            cfg.db = db.into();
        }
    }

    if let Some(clas) = matches.values_of("cla") {
        for cla in clas {
            let cla_split: Vec<&str> = cla.split(':').collect();
            if dtn7::cla::convergence_layer_agents().contains(&cla_split[0]) {
                cfg.clas.push(cla.to_string());
            }
        }
    }
    if let Some(services) = matches.values_of("service") {
        for service in services {
            let service_split: Vec<&str> = service.split(':').collect();
            let tag: u8 = service_split[0]
                .parse()
                .expect("Couldn't parse tag properly");
            if cfg.services.contains_key(&tag) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "Tags must be unique. You tried to use tag {} multiple times.",
                        tag
                    ),
                ));
            }
            let payload = String::from(service_split[1]);
            cfg.services.insert(tag, payload);
        }
    }
    if let Some(destinations) = matches.values_of("discoverydestination") {
        for destination in destinations {
            cfg.add_destination(String::from(destination))
                .expect("Encountered an error while parsing discovery address to config");
        }
    }
    cfg.check_destinations()
        .expect("Encountered an error while checking for the existence of discovery addresses");
    if let Some(statics) = matches.values_of("staticpeer") {
        for s in statics {
            cfg.statics.push(dtn7::core::helpers::parse_peer_url(s));
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
    start_dtnd(cfg).await.unwrap();
    Ok(())
}
