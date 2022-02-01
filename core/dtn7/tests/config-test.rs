use config::{Config, File};

#[test]
fn config_test() {
    let mut s = Config::default();

    // Start off by merging in the "default" configuration file
    s.merge(File::new(
        "../../examples/dtn7.toml.example",
        config::FileFormat::Toml,
    ))
    .unwrap();
    println!("{:?}", s);

    println!("debug: {:?}", s.get_bool("debug").unwrap_or(false));
    println!("nodeid: {:?}", s.get_str("nodeid").unwrap());
    println!("routing: {:?}", s.get_str("routing").unwrap());
    println!("janitor: {:?}", s.get_str("core.janitor").unwrap());
    println!("workdir: {:?}", s.get_str("workdir").unwrap());
    println!("db: {:?}", s.get_str("db").unwrap());

    println!(
        "discovery-interval: {:?}",
        s.get_str("discovery.interval").unwrap()
    );
    println!(
        "discovery-peer-timeout: {:?}",
        s.get_str("discovery.peer-timeout").unwrap()
    );

    let peers = s.get_array("statics.peers");

    for m in peers.unwrap().iter() {
        println!("Peer: {:?}", m.clone().into_str().unwrap());
    }

    let endpoints = s.get_table("endpoints.local");

    for (_k, v) in endpoints.unwrap().iter() {
        println!("EID: {:?}", v.clone().into_str().unwrap());
    }

    let clas = s.get_table("convergencylayers.cla");
    for (_k, v) in clas.unwrap().iter() {
        let tab = v.clone().into_table().unwrap();
        println!("CLA: {:?}", tab["id"].clone().into_str().unwrap());
    }
}
