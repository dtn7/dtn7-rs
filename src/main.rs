use clap::{App, Arg, SubCommand};
use dtn7::cl::dummy_cl::*;
use dtn7::cl::stcp::*;
use dtn7::core::application_agent::ApplicationAgentData;
use dtn7::core::core::DtnCore;
use dtn7::dtnd::daemon::*;
use log::{info, trace, warn};
use pretty_env_logger;

fn main() {
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const AUTHORS: &'static str = env!("CARGO_PKG_AUTHORS");

    let matches = App::new("dtn7-rs")
        .version(VERSION)
        .author(AUTHORS)
        .about("A simple Bundle Protocol 7 Daemon for Delay Tolerant Networking")
        .get_matches();

    std::env::set_var("RUST_LOG", "dtn7=debug,dtnd=debug");
    pretty_env_logger::init_timed();

    let mut core = DtnCore::new("node1".to_string());

    println!("Local Application Agent EIDs:");
    dbg!(core.eids());

    //core.unregister_application_agent(aad2);
    //dbg!(core.eids());

    let dcl = DummyConversionLayer::new();
    core.cl_list.push(Box::new(dcl));
    let stcp = StcpConversionLayer::new();
    core.cl_list.push(Box::new(stcp));
    info!("starting dtnd");
    start_dtnd(core);
}
