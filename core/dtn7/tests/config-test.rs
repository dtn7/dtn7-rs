use config::{Config, File};

#[test]
fn config_test() {
    // Start off by merging in the "default" configuration file
    let s = Config::builder()
        .add_source(Config::default())
        .add_source(File::new(
            "../../examples/dtn7.toml.example",
            config::FileFormat::Toml,
        ))
        .build()
        .unwrap();

    println!("{:?}", s);

    println!("debug: {:?}", s.get_bool("debug").unwrap_or(false));
    println!("nodeid: {:?}", s.get_string("nodeid").unwrap());
    println!("routing: {:?}", s.get_string("routing.strategy").unwrap());
    println!("janitor: {:?}", s.get_string("core.janitor").unwrap());
    println!("workdir: {:?}", s.get_string("workdir").unwrap());
    println!("db: {:?}", s.get_string("db").unwrap());

    println!(
        "discovery-interval: {:?}",
        s.get_string("discovery.interval").unwrap()
    );
    println!(
        "discovery-peer-timeout: {:?}",
        s.get_string("discovery.peer-timeout").unwrap()
    );

    let peers = s.get_array("statics.peers");

    for m in peers.unwrap().iter() {
        println!("Peer: {:?}", m.clone().into_string().unwrap());
    }

    let endpoints = s.get_table("endpoints.local");

    for v in endpoints.unwrap().values() {
        println!("EID: {:?}", v.clone().into_string().unwrap());
    }

    let clas = s.get_table("convergencylayers.cla");
    for v in clas.unwrap().values() {
        let tab = v.clone().into_table().unwrap();
        println!("CLA: {:?}", tab["id"].clone().into_string().unwrap());
    }
}

fn load_config(name: &str, content: &str) -> dtn7::DtnConfig {
    let path = std::env::temp_dir().join(format!("dtn7-config-test-{}.toml", name));
    std::fs::write(&path, content).unwrap();
    let config = dtn7::DtnConfig::from(path.clone());
    std::fs::remove_file(path).unwrap();
    config
}

#[test]
fn config_keys_from_example_are_used() {
    let config = load_config(
        "documented-keys",
        "generate-status-reports = true\nstrict-validation = true\n",
    );
    assert!(config.generate_status_reports);
    assert!(config.strict_validation);

    // still supported for configurations of older versions
    let config = load_config("legacy-keys", "generate_status_reports = true\n");
    assert!(config.generate_status_reports);

    let config = load_config("defaults", "");
    assert!(!config.generate_status_reports);
    assert!(!config.strict_validation);
}
