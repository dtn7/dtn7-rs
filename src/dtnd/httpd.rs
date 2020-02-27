use crate::core::application_agent::SimpleApplicationAgent;
use crate::core::helpers::rnd_peer;
use crate::peer_find_by_remote;
use crate::routing::RoutingNotifcation;
use crate::CONFIG;
use crate::DTNCORE;
use crate::PEERS;
use crate::STATS;
use crate::STORE;
use actix_web::HttpResponse;
use actix_web::{get, post, web, App, HttpRequest, HttpServer, Responder, Result};
use anyhow::anyhow;
use bp7::dtntime::CreationTimestamp;
use bp7::helpers::rnd_bundle;
use futures::StreamExt;
use log::{debug, error, info};
use std::convert::TryFrom;

#[get("/")]
async fn index() -> &'static str {
    "dtn7 ctrl interface"
}

#[get("/status/nodeid")]
async fn status_node_id() -> String {
    (*CONFIG.lock()).host_eid.to_string()
}

#[get("/status/eids")]
async fn status_eids() -> String {
    serde_json::to_string_pretty(&(*DTNCORE.lock()).eids()).unwrap()
}
#[get("/status/bundles")]
async fn status_bundles() -> String {
    serde_json::to_string_pretty(&(*DTNCORE.lock()).bundles()).unwrap()
}
#[get("/status/bundles_dest")]
async fn status_bundles_dest() -> String {
    serde_json::to_string_pretty(&(*DTNCORE.lock()).bundle_names()).unwrap()
}
#[get("/status/store")]
async fn status_store() -> String {
    serde_json::to_string_pretty(&(*STORE.lock()).bundles_status()).unwrap()
}
#[get("/status/peers")]
async fn status_peers() -> String {
    let peers = &(*PEERS.lock()).clone();
    serde_json::to_string_pretty(&peers).unwrap()
}
#[get("/status/info")]
async fn status_info() -> String {
    let stats = &(*STATS.lock()).clone();
    serde_json::to_string_pretty(&stats).unwrap()
}

#[get("/debug/rnd_bundle")]
async fn debug_rnd_bundle() -> String {
    println!("generating debug bundle");
    let b = rnd_bundle(CreationTimestamp::now());
    let res = b.id();
    crate::core::processing::send_bundle(b);
    res
}

#[get("/debug/rnd_peer")]
async fn debug_rnd_peer() -> String {
    println!("generating debug peer");
    let p = rnd_peer();
    let res = serde_json::to_string_pretty(&p).unwrap();
    (*PEERS.lock()).insert(p.eid.node_part().unwrap_or_default(), p);
    res
}
#[get("/insert")]
async fn insert_get(req: HttpRequest) -> Result<String> {
    debug!("REQ: {:?}", req);
    debug!("BUNDLE: {}", req.query_string());
    let bundle = req.query_string();

    if bundle.chars().all(char::is_alphanumeric) {
        if let Ok(hexstr) = bp7::helpers::unhexify(&bundle) {
            let b_len = hexstr.len();
            if let Ok(bndl) = bp7::Bundle::try_from(hexstr) {
                debug!(
                    "Sending bundle {} to {}",
                    bndl.id(),
                    bndl.primary.destination
                );

                crate::core::processing::send_bundle(bndl);
                Ok(format!("Sent {} bytes", b_len))
            } else {
                Err(actix_web::error::ErrorBadRequest(anyhow!(
                    "Error decoding bundle!"
                )))
            }
        } else {
            Err(actix_web::error::ErrorBadRequest(anyhow!(
                "Error parsing bundle!"
            )))
        }
    } else {
        Err(actix_web::error::ErrorBadRequest(anyhow!(
            "Not a valid bundle hex string!"
        )))
    }
}
#[post("/insert")]
async fn insert_post(mut body: web::Payload) -> Result<String> {
    let mut bytes = web::BytesMut::new();
    while let Some(item) = body.next().await {
        bytes.extend_from_slice(&item?);
    }
    let b_len = bytes.len();
    debug!("Received: {:?}", b_len);
    if let Ok(bndl) = bp7::Bundle::try_from(bytes.to_vec()) {
        debug!(
            "Sending bundle {} to {}",
            bndl.id(),
            bndl.primary.destination
        );

        crate::core::processing::send_bundle(bndl);
        Ok(format!("Sent {} bytes", b_len))
    } else {
        Err(actix_web::error::ErrorBadRequest(anyhow!(
            "Error decoding bundle!"
        )))
    }
}

#[post("/push")]
async fn push_post(req: HttpRequest, mut body: web::Payload) -> Result<String> {
    let mut bytes = web::BytesMut::new();
    while let Some(item) = body.next().await {
        bytes.extend_from_slice(&item?);
    }
    let b_len = bytes.len();
    debug!("Received: {:?}", b_len);
    if let Ok(bndl) = bp7::Bundle::try_from(bytes.to_vec()) {
        debug!("Received bundle {}", bndl.id());
        if let Some(peer_addr) = req.peer_addr() {
            if let Some(node_name) = peer_find_by_remote(&peer_addr.ip()) {
                (*DTNCORE.lock())
                    .routing_agent
                    .notify(RoutingNotifcation::IncomingBundle(&bndl.id(), &node_name));
            }
        }
        crate::core::processing::receive(bndl.into());
        Ok(format!("Received {} bytes", b_len))
    } else {
        Err(actix_web::error::ErrorBadRequest(anyhow!(
            "Error decoding bundle!"
        )))
    }
}

#[get("/register")]
async fn register(req: HttpRequest) -> Result<String> {
    let path = req.query_string();
    // TODO: support non-node-specific EIDs
    if path.chars().all(char::is_alphanumeric) {
        let eid = format!("dtn://{}/{}", (*CONFIG.lock()).nodeid, path);
        (*DTNCORE.lock())
            .register_application_agent(SimpleApplicationAgent::new_with(eid.clone().into()));
        Ok(format!("Registered {}", eid))
    } else {
        Err(actix_web::error::ErrorBadRequest(anyhow!(
            "Malformed endpoint path, only alphanumeric strings allowed!"
        )))
    }
}

#[get("/unregister")]
async fn unregister(req: HttpRequest) -> Result<String> {
    let path = req.query_string();
    // TODO: support non-node-specific EIDs
    if path.chars().all(char::is_alphanumeric) {
        let eid = format!("dtn://{}/{}", (*CONFIG.lock()).nodeid, path);
        (*DTNCORE.lock())
            .unregister_application_agent(SimpleApplicationAgent::new_with(eid.clone().into()));
        Ok(format!("Unregistered {}", eid))
    } else {
        Err(actix_web::error::ErrorBadRequest(anyhow!(
            "Malformed endpoint path, only alphanumeric strings allowed!"
        )))
    }
}

#[get("/endpoint")]
async fn endpoint(req: HttpRequest) -> Result<HttpResponse> {
    let path = req.query_string();
    if path.chars().all(char::is_alphanumeric) {
        let eid = format!("dtn://{}/{}", (*CONFIG.lock()).nodeid, path); // TODO: support non-node-specific EIDs
        if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&eid.into()) {
            if let Some(mut bundle) = aa.pop() {
                let cbor_bundle = bundle.to_cbor();
                Ok(HttpResponse::Ok()
                    .content_type("application/octet-stream")
                    .body(cbor_bundle))
            } else {
                Ok(HttpResponse::Ok()
                    .content_type("plain/text")
                    .body("Nothing to receive"))
            }
        } else {
            //*response.status_mut() = StatusCode::NOT_FOUND;
            Err(actix_web::error::ErrorBadRequest(anyhow!(
                "No such endpoint registered!"
            )))
        }
    } else {
        Err(actix_web::error::ErrorBadRequest(anyhow!(
            "Malformed endpoint path, only alphanumeric strings allowed!"
        )))
    }
}
#[get("/endpoint.hex")]
async fn endpoint_hex(req: HttpRequest) -> Result<String> {
    let path = req.query_string();
    if path.chars().all(char::is_alphanumeric) {
        let eid = format!("dtn://{}/{}", (*CONFIG.lock()).nodeid, path); // TODO: support non-node-specific EIDs
        if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&eid.into()) {
            if let Some(mut bundle) = aa.pop() {
                Ok(bp7::helpers::hexify(&bundle.to_cbor()))
            } else {
                Ok("Nothing to receive".to_string())
            }
        } else {
            //*response.status_mut() = StatusCode::NOT_FOUND;
            Err(actix_web::error::ErrorBadRequest(anyhow!(
                "No such endpoint registered!"
            )))
        }
    } else {
        Err(actix_web::error::ErrorBadRequest(anyhow!(
            "Malformed endpoint path, only alphanumeric strings allowed!"
        )))
    }
}

#[get("/download")]
async fn download(req: HttpRequest) -> Result<HttpResponse> {
    let bid = req.query_string();
    if let Some(bundlepack) = (*STORE.lock()).get(&bid) {
        let cbor_bundle = bundlepack.bundle.clone().to_cbor();
        Ok(HttpResponse::Ok()
            .content_type("application/octet-stream")
            .body(cbor_bundle))
    } else {
        Err(actix_web::error::ErrorBadRequest(anyhow!(
            "Bundle not found"
        )))
    }
}
#[get("/download.hex")]
async fn download_hex(req: HttpRequest) -> Result<String> {
    let bid = req.query_string();
    if let Some(bundlepack) = (*STORE.lock()).get(&bid) {
        Ok(bp7::helpers::hexify(&bundlepack.bundle.clone().to_cbor()))
    } else {
        Err(actix_web::error::ErrorBadRequest(anyhow!(
            "Bundle not found"
        )))
    }
}

pub async fn spawn_httpd() -> std::io::Result<()> {
    let port = (*CONFIG.lock()).webport;
    HttpServer::new(|| {
        App::new()
            .service(index)
            .service(status_node_id)
            .service(status_eids)
            .service(status_bundles)
            .service(status_bundles_dest)
            .service(status_store)
            .service(status_peers)
            .service(status_info)
            .service(debug_rnd_bundle)
            .service(debug_rnd_peer)
            .service(insert_get)
            .service(insert_post)
            .service(push_post)
            .service(register)
            .service(unregister)
            .service(endpoint)
            .service(endpoint_hex)
            .service(download)
            .service(download_hex)
    })
    .bind(&format!("0.0.0.0:{}", port))?
    .run()
    .await
}
