use crate::core::application_agent::SimpleApplicationAgent;
use crate::core::helpers::rnd_peer;
use crate::CONFIG;
use crate::DTNCORE;
use crate::PEERS;
use crate::STATS;
use crate::STORE;
use anyhow::{anyhow, Error, Result};
use bp7::dtntime::CreationTimestamp;
use bp7::helpers::rnd_bundle;
use log::{debug, error, info};
use rocket::config::{Config, Environment};
use rocket::State;
use rocket::*;
use std::io::prelude::*;

#[get("/")]
fn index() -> &'static str {
    "dtn7 ctrl interface"
}

#[get("/status/nodeid")]
fn status_node_id() -> String {
    (*CONFIG.lock()).host_eid.to_string()
}
#[get("/status/eids")]
fn status_eids() -> String {
    serde_json::to_string_pretty(&(*DTNCORE.lock()).eids()).unwrap()
}
#[get("/status/bundles")]
fn status_bundles() -> String {
    serde_json::to_string_pretty(&(*DTNCORE.lock()).bundles()).unwrap()
}
#[get("/status/store")]
fn status_store() -> String {
    serde_json::to_string_pretty(&(*STORE.lock()).bundles_status()).unwrap()
}
#[get("/status/peers")]
fn status_peers() -> String {
    let peers = &(*PEERS.lock()).clone();
    serde_json::to_string_pretty(&peers).unwrap()
}
#[get("/status/info")]
fn status_info() -> String {
    let stats = &(*STATS.lock()).clone();
    serde_json::to_string_pretty(&stats).unwrap()
}

#[get("/debug/rnd_bundle")]
fn debug_rnd_bundle() -> String {
    println!("generating debug bundle");
    let b = rnd_bundle(CreationTimestamp::now());
    let res = b.id();
    crate::core::processing::send_bundle(b);
    res
}

#[get("/debug/rnd_peer")]
fn debug_rnd_peer() -> String {
    println!("generating debug peer");
    let p = rnd_peer();
    let res = serde_json::to_string_pretty(&p).unwrap();
    (*PEERS.lock()).insert(p.eid.node_part().unwrap_or_default(), p);
    res
}

#[get("/send?<bundle>")]
fn GET_send(bundle: String) -> Result<String> {
    if bundle.chars().all(char::is_alphanumeric) {
        if let Ok(hexstr) = bp7::helpers::unhexify(&bundle) {
            let b_len = hexstr.len();
            let bndl = bp7::Bundle::from(hexstr);
            debug!(
                "Sending bundle {} to {}",
                bndl.id(),
                bndl.primary.destination
            );
            {
                crate::core::processing::send_bundle(bndl);
            }
            Ok(format!("Sent {} bytes", b_len))
        } else {
            Err(anyhow!("Error parsing bundle!"))
        }
    } else {
        Err(anyhow!("Not a valid bundle hex string!"))
    }
}
#[post("/send", data = "<data>")]
fn POST_send(data: Data) -> Result<String> {
    let mut binbundle = Vec::new();
    let b_len = data.open().read_to_end(&mut binbundle)?;
    let bndl = bp7::Bundle::from(binbundle);
    debug!(
        "Sending bundle {} to {}",
        bndl.id(),
        bndl.primary.destination
    );
    {
        crate::core::processing::send_bundle(bndl);
    }
    Ok(format!("Sent {} bytes", b_len))
}

#[get("/register?<path>")]
fn register(path: String) -> Result<String> {
    // TODO: support non-node-specific EIDs
    if path.chars().all(char::is_alphanumeric) {
        let eid = format!("dtn://{}/{}", (*CONFIG.lock()).nodeid, path);
        (*DTNCORE.lock())
            .register_application_agent(SimpleApplicationAgent::new_with(eid.clone().into()));
        Ok(format!("Registered {}", eid))
    } else {
        Err(anyhow!(
            "Malformed endpoint path, only alphanumeric strings allowed!"
        ))
    }
}

#[get("/unregister?<path>")]
fn unregister(path: String) -> Result<String> {
    // TODO: support non-node-specific EIDs
    if path.chars().all(char::is_alphanumeric) {
        let eid = format!("dtn://{}/{}", (*CONFIG.lock()).nodeid, path);
        (*DTNCORE.lock())
            .unregister_application_agent(SimpleApplicationAgent::new_with(eid.clone().into()));
        Ok(format!("Unregistered {}", eid))
    } else {
        Err(anyhow!(
            "Malformed endpoint path, only alphanumeric strings allowed!"
        ))
    }
}

#[get("/endpoint?<path>")]
fn endpoint(path: String) -> Result<Vec<u8>> {
    if path.chars().all(char::is_alphanumeric) {
        let eid = format!("dtn://{}/{}", (*CONFIG.lock()).nodeid, path); // TODO: support non-node-specific EIDs
        if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&eid.into()) {
            if let Some(mut bundle) = aa.pop() {
                Ok(bundle.to_cbor())
            } else {
                Err(anyhow!("Nothing to receive"))
            }
        } else {
            //*response.status_mut() = StatusCode::NOT_FOUND;
            Err(anyhow!("No such endpoint registered!"))
        }
    } else {
        Err(anyhow!(
            "Malformed endpoint path, only alphanumeric strings allowed!"
        ))
    }
}
#[get("/endpoint.hex?<path>")]
fn endpoint_hex(path: String) -> Result<String> {
    if path.chars().all(char::is_alphanumeric) {
        let eid = format!("dtn://{}/{}", (*CONFIG.lock()).nodeid, path); // TODO: support non-node-specific EIDs
        if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&eid.into()) {
            if let Some(mut bundle) = aa.pop() {
                Ok(bp7::helpers::hexify(&bundle.to_cbor()))
            } else {
                Err(anyhow!("Nothing to receive"))
            }
        } else {
            //*response.status_mut() = StatusCode::NOT_FOUND;
            Err(anyhow!("No such endpoint registered!"))
        }
    } else {
        Err(anyhow!(
            "Malformed endpoint path, only alphanumeric strings allowed!"
        ))
    }
}

pub fn spawn_rest() {
    let port = (*CONFIG.lock()).webport;

    let config = Config::build(Environment::Staging)
        .address("127.0.0.1")
        .port(port)
        .workers(12)
        .unwrap();

    let _handler = std::thread::spawn(|| {
        // thread code
        rocket::custom(config)
            .mount(
                "/",
                routes![
                    index,
                    status_node_id,
                    status_eids,
                    status_bundles,
                    status_peers,
                    status_store,
                    status_info,
                    debug_rnd_bundle,
                    debug_rnd_peer,
                    GET_send,
                    POST_send,
                    register,
                    unregister,
                    endpoint,
                    endpoint_hex,
                ],
            )
            .launch();
    });
}
