use dtn7::dtnconfig::DtnConfig;
use serde::Serialize;
use tinytemplate::TinyTemplate;

#[derive(Serialize)]
struct Context<'a> {
    config: &'a DtnConfig,
    janitor: String,
    announcement: String,
    timeout: String,
    num_peers: u64,
    num_bundles: u64,
    clas: Vec<String>,
}
#[test]
fn template_test() {
    let template_str = include_str!("../webroot/index.html");
    let mut tt = TinyTemplate::new();
    tt.add_template("index", template_str).unwrap();
    let cfg = DtnConfig::new();
    let context = Context {
        config: &cfg,
        janitor: "2s".to_owned(),
        announcement: "10s".to_owned(),
        timeout: "20s".to_owned(),
        num_peers: 4,
        num_bundles: 10,
        clas: vec![],
    };

    let rendered = tt.render("index", &context).unwrap();
    println!("{}", rendered);
}
