use crate::core::application_agent::ApplicationAgent;
use crate::core::application_agent::SimpleApplicationAgent;
use crate::core::bundlepack::Constraint;
use crate::core::helpers::rnd_peer;
use crate::core::peer::PeerType;
use crate::core::store::BundleStore;
use crate::peers_count;
use crate::DtnConfig;
use crate::CONFIG;
use crate::DTNCORE;
use crate::PEERS;
use crate::STATS;
use crate::STORE;
use anyhow::Result;
use async_trait::async_trait;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::Html;
use axum::{
    extract::{self, connect_info::ConnectInfo, extractor_middleware, RequestParts},
    routing::{get, post},
    Router,
};
use bp7::dtntime::CreationTimestamp;
use bp7::flags::BlockControlFlags;
use bp7::flags::BundleControlFlags;
use bp7::helpers::rnd_bundle;
use bp7::EndpointID;
use http::StatusCode;
use humansize::{file_size_opts, FileSize};
use log::{debug, info, warn};
use serde::Serialize;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::net::SocketAddr;
use tinytemplate::TinyTemplate;
/*

#[get("/ws", guard = "fn_guard_localhost")]
async fn ws_application_agent(
    req: HttpRequest,
    stream: web::Payload,
) -> Result<HttpResponse, Error> {
    ws::start(WsAASession::new(), &req, stream)
}
*/

struct RequireLocalhost;

#[async_trait]
impl<B> extract::FromRequest<B> for RequireLocalhost
where
    B: Send,
{
    type Rejection = StatusCode;

    async fn from_request(conn: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        if (*CONFIG.lock()).unsafe_httpd {
            return Ok(Self);
        }
        if let Some(ext) = conn.extensions() {
            if let Some(ConnectInfo(addr)) = ext.get::<ConnectInfo<SocketAddr>>() {
                if addr.ip().is_loopback() {
                    return Ok(Self);
                } else if let std::net::IpAddr::V6(ipv6) = addr.ip() {
                    // workaround for bug in std when handling IPv4 in IPv6 addresses
                    if let Some(ipv4) = ipv6.to_ipv4() {
                        if ipv4.is_loopback() {
                            return Ok(Self);
                        }
                    }
                }
            }
        }

        Err(StatusCode::FORBIDDEN)
    }
}

// Begin of web UI specific structs

#[derive(Serialize)]
struct IndexContext<'a> {
    config: &'a DtnConfig,
    janitor: String,
    announcement: String,
    timeout: String,
    num_peers: usize,
    num_bundles: usize,
}

#[derive(Serialize)]
struct PeersContext<'a> {
    config: &'a DtnConfig,
    peers: &'a [PeerEntry],
}
#[derive(Serialize)]
struct PeerEntry {
    name: String,
    con_type: PeerType,
    last: String,
}

#[derive(Serialize)]
struct BundleInfo {
    id: String,
    size: String,
}

#[derive(Serialize)]
struct BundlesContext<'a> {
    config: &'a DtnConfig,
    bundles: &'a [BundleInfo],
}
#[derive(Serialize)]
struct BundleEntry {
    bid: String,
    src: String,
    dst: String,
}

// End of web UI specific structs

//#[get("/")]
async fn index() -> Html<String> {
    // "dtn7 ctrl interface"
    let template_str = include_str!("../../webroot/index.html");
    let mut tt = TinyTemplate::new();
    tt.add_template("index", template_str)
        .expect("error adding template");
    let announcement =
        humantime::format_duration((*CONFIG.lock()).announcement_interval).to_string();
    let janitor = humantime::format_duration((*CONFIG.lock()).janitor_interval).to_string();
    let timeout = humantime::format_duration((*CONFIG.lock()).peer_timeout).to_string();
    let context = IndexContext {
        config: &(*CONFIG.lock()),
        announcement,
        janitor,
        timeout,
        num_peers: peers_count(),
        num_bundles: (*DTNCORE.lock()).bundle_count(),
    };

    let rendered = tt
        .render("index", &context)
        .expect("error rendering template");
    Html(rendered)
}

//#[get("/peers")]
async fn web_peers() -> Html<String> {
    // "dtn7 ctrl interface"
    let template_str = include_str!("../../webroot/peers.html");
    let mut tt = TinyTemplate::new();
    tt.add_template("peers", template_str)
        .expect("error adding template");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time went backwards")
        .as_secs();
    let peers_vec: Vec<PeerEntry> = (*PEERS.lock())
        .values()
        .map(|p| {
            let time_since = if p.con_type == PeerType::Dynamic {
                humantime::format_duration(std::time::Duration::new(now - p.last_contact, 0))
                    .to_string()
            } else {
                "n/a".to_string()
            };
            PeerEntry {
                name: p.eid.to_string(),
                con_type: p.con_type,
                last: time_since,
            }
        })
        .collect();

    let context = PeersContext {
        config: &(*CONFIG.lock()),
        peers: peers_vec.as_slice(),
    };
    //let peers_vec: Vec<&DtnPeer> = (*PEERS.lock()).values().collect();
    let rendered = tt
        .render("peers", &context)
        .expect("error rendering template");
    Html(rendered)
}

//#[get("/bundles")]
async fn web_bundles() -> Html<String> {
    // "dtn7 ctrl interface"
    let template_str = include_str!("../../webroot/bundles.html");
    let mut tt = TinyTemplate::new();
    tt.add_template("bundles", template_str)
        .expect("error adding template");
    let bundles_vec: Vec<BundleInfo> = (STORE.lock())
        .bundles()
        .iter()
        .filter(|bp| !bp.has_constraint(Constraint::Deleted))
        .map(|bp| BundleInfo {
            id: bp.id.to_string(),
            size: bp.size.file_size(file_size_opts::DECIMAL).unwrap(),
        })
        .collect();
    let context = BundlesContext {
        config: &(*CONFIG.lock()),
        bundles: bundles_vec.as_slice(),
    };
    //let peers_vec: Vec<&DtnPeer> = (*PEERS.lock()).values().collect();
    let rendered = tt
        .render("bundles", &context)
        .expect("error rendering template");
    Html(rendered)
}

//#[get("/status/nodeid")]
async fn status_node_id() -> String {
    (*CONFIG.lock()).host_eid.to_string()
}

//#[get("/status/eids")]
async fn status_eids() -> String {
    serde_json::to_string_pretty(&(*DTNCORE.lock()).eids()).unwrap()
}
//#[get("/status/bundles")]
async fn status_bundles() -> String {
    let bids: Vec<String> = (*STORE.lock())
        .bundles()
        .iter()
        .filter(|bp| !bp.has_constraint(Constraint::Deleted))
        .map(|bp| bp.id.to_string())
        .collect();
    serde_json::to_string_pretty(&bids).unwrap()
}
//#[get("/status/bundles_dest")]
async fn status_bundles_dest() -> String {
    serde_json::to_string_pretty(&(*DTNCORE.lock()).bundle_names()).unwrap()
}
//#[get("/status/store", guard = "fn_guard_localhost")]
async fn status_store() -> String {
    serde_json::to_string_pretty(&(*STORE.lock()).bundles_status()).unwrap()
}
//#[get("/status/peers")]
async fn status_peers() -> String {
    let peers = &(*PEERS.lock()).clone();
    serde_json::to_string_pretty(&peers).unwrap()
}
//#[get("/status/info")]
async fn status_info() -> String {
    let stats = &(*STATS.lock()).clone();
    serde_json::to_string_pretty(&stats).unwrap()
}
//#[get("/cts", guard = "fn_guard_localhost")]
async fn get_creation_timestamp() -> String {
    let cts = bp7::CreationTimestamp::now();
    serde_json::to_string(&cts).unwrap()
}

//#[get("/debug/rnd_bundle", guard = "fn_guard_localhost")]
async fn debug_rnd_bundle() -> String {
    debug!("inserting debug bundle");
    let b = rnd_bundle(CreationTimestamp::now());
    let res = b.id();
    crate::core::processing::send_bundle(b).await;
    res
}

//#[get("/debug/rnd_peer", guard = "fn_guard_localhost")]
async fn debug_rnd_peer() -> String {
    debug!("inserting debug peer");
    let p = rnd_peer();
    let res = serde_json::to_string_pretty(&p).unwrap();
    (*PEERS.lock()).insert(p.eid.node().unwrap_or_default(), p);
    res
}

//#[get("/insert", guard = "fn_guard_localhost")]
async fn insert_get(extract::RawQuery(query): extract::RawQuery) -> Result<String, StatusCode> {
    debug!("REQ: {:?}", query);
    if query.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let bundle = query.unwrap();
    debug!("BUNDLE: {}", bundle);

    if bundle.chars().all(char::is_alphanumeric) {
        if let Ok(hexstr) = bp7::helpers::unhexify(&bundle) {
            let b_len = hexstr.len();
            if let Ok(bndl) = bp7::Bundle::try_from(hexstr) {
                debug!(
                    "Sending bundle {} to {}",
                    bndl.id(),
                    bndl.primary.destination
                );

                crate::core::processing::send_bundle(bndl).await;
                Ok(format!("Sent {} bytes", b_len))
            } else {
                Err(StatusCode::BAD_REQUEST)
            }
        } else {
            Err(StatusCode::BAD_REQUEST)
        }
    } else {
        Err(StatusCode::BAD_REQUEST)
    }
}

//#[post("/insert", guard = "fn_guard_localhost")]
async fn insert_post(body: bytes::Bytes) -> Result<String, (StatusCode, &'static str)> {
    let bytes = body.to_vec();
    let b_len = bytes.len();
    debug!("Received: {:?}", b_len);
    if let Ok(bndl) = bp7::Bundle::try_from(bytes.to_vec()) {
        debug!(
            "Sending bundle {} to {}",
            bndl.id(),
            bndl.primary.destination
        );

        crate::core::processing::send_bundle(bndl).await;
        Ok(format!("Sent {} bytes", b_len))
    } else {
        Err((StatusCode::BAD_REQUEST, "Error decoding bundle!"))
    }
}

//#[post("/send", guard = "fn_guard_localhost")]
async fn send_post(
    query_params: extract::Query<HashMap<String, String>>,
    body: bytes::Bytes,
) -> Result<String, (StatusCode, &'static str)> {
    let mut dst: EndpointID = EndpointID::none();
    let mut lifetime = std::time::Duration::from_secs(60 * 60);
    for (k, v) in query_params.iter() {
        if k == "dst" {
            dst = v.as_str().try_into().unwrap();
        } else if k == "lifetime" {
            if let Ok(dur) = humantime::parse_duration(v) {
                lifetime = dur;
            }
        }
    }
    if dst == EndpointID::none() {
        return Err((StatusCode::BAD_REQUEST, "Missing destination endpoint id!"));
    }
    let src = (*CONFIG.lock()).host_eid.clone();
    let flags = BundleControlFlags::BUNDLE_MUST_NOT_FRAGMENTED
        | BundleControlFlags::BUNDLE_STATUS_REQUEST_DELIVERY;
    let pblock = bp7::primary::PrimaryBlockBuilder::default()
        .bundle_control_flags(flags.bits())
        .destination(dst)
        .source(src.clone())
        .report_to(src)
        .creation_timestamp(CreationTimestamp::now())
        .lifetime(lifetime)
        .build()
        .unwrap();

    let bytes = body.to_vec();

    let b_len = bytes.len();
    debug!("Received for sending: {:?}", b_len);
    let mut bndl = bp7::bundle::BundleBuilder::default()
        .primary(pblock)
        .canonicals(vec![
            bp7::canonical::new_payload_block(BlockControlFlags::empty(), bytes.to_vec()),
            bp7::canonical::new_hop_count_block(2, BlockControlFlags::empty(), 32),
        ])
        .build()
        .unwrap();
    bndl.set_crc(bp7::crc::CRC_NO);

    debug!(
        "Sending bundle {} to {}",
        bndl.id(),
        bndl.primary.destination
    );

    crate::core::processing::send_bundle(bndl).await;
    Ok(format!("Sent payload with {} bytes", b_len))
}

//#[post("/push")]
async fn push_post(body: bytes::Bytes) -> Result<String, (StatusCode, String)> {
    let bytes = body.to_vec();

    let b_len = bytes.len();
    debug!("Received: {:?}", b_len);
    if let Ok(bndl) = bp7::Bundle::try_from(bytes.to_vec()) {
        info!("Received bundle {}", bndl.id());
        if let Err(err) = crate::core::processing::receive(bndl).await {
            warn!("Error processing bundle: {}", err);
            Err((
                StatusCode::BAD_REQUEST,
                format!("Error processing bundle: {}", err),
            ))
        } else {
            Ok(format!("Received {} bytes", b_len))
        }
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            "Error decoding bundle!".to_string(),
        ))
    }
}

//#[get("/register", guard = "fn_guard_localhost")]
async fn register(
    extract::RawQuery(query): extract::RawQuery,
) -> Result<String, (StatusCode, &'static str)> {
    if query.is_none() {
        return Err((StatusCode::BAD_REQUEST, "missing query parameter"));
    }
    let path = query.unwrap();
    if path.chars().all(char::is_alphanumeric) {
        // without url scheme assume a local DTN service name
        let host_eid = (*CONFIG.lock()).host_eid.clone();
        let eid = host_eid
            .new_endpoint(&path)
            .expect("Error constructing new endpoint");
        (*DTNCORE.lock())
            .register_application_agent(SimpleApplicationAgent::with(eid.clone()).into());
        Ok(format!("Registered {}", eid))
    } else if let Ok(eid) = EndpointID::try_from(path) {
        // fully qualified EID, can be non-singleton endpoint
        (*DTNCORE.lock())
            .register_application_agent(SimpleApplicationAgent::with(eid.clone()).into());
        Ok(format!("Registered URI: {}", eid))
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            "Malformed endpoint path, only alphanumeric strings or endpoint URIs are allowed!",
        ))
    }
}

//#[get("/unregister", guard = "fn_guard_localhost")]
async fn unregister(
    extract::RawQuery(query): extract::RawQuery,
) -> Result<String, (StatusCode, &'static str)> {
    if query.is_none() {
        return Err((StatusCode::BAD_REQUEST, "missing query parameter"));
    }
    let path = query.unwrap();
    if path.chars().all(char::is_alphanumeric) {
        let host_eid = (*CONFIG.lock()).host_eid.clone();
        let eid = host_eid
            .new_endpoint(&path)
            .expect("Error constructing new endpoint");

        (*DTNCORE.lock())
            .unregister_application_agent(SimpleApplicationAgent::with(eid.clone()).into());
        Ok(format!("Unregistered {}", eid))
    } else if let Ok(eid) = EndpointID::try_from(path) {
        (*DTNCORE.lock())
            .unregister_application_agent(SimpleApplicationAgent::with(eid.clone()).into());
        Ok(format!("Unregistered URI: {}", eid))
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            "Malformed endpoint path, only alphanumeric strings or endpoint URIs are allowed!",
        ))
    }
}

//#[get("/endpoint", guard = "fn_guard_localhost")]
async fn endpoint(
    extract::RawQuery(query): extract::RawQuery,
) -> Result<Vec<u8>, (StatusCode, &'static str)> {
    if query.is_none() {
        return Err((StatusCode::BAD_REQUEST, "missing query parameter"));
    }
    let path = query.unwrap();
    if path.chars().all(char::is_alphanumeric) {
        let host_eid = (*CONFIG.lock()).host_eid.clone();
        let eid = host_eid
            .new_endpoint(&path)
            .expect("Error constructing new endpoint"); // TODO: support non-node-specific EIDs
        if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&eid) {
            if let Some(mut bundle) = aa.pop() {
                let cbor_bundle = bundle.to_cbor();
                Ok(cbor_bundle)
            } else {
                Ok("Nothing to receive".as_bytes().to_vec())
            }
        } else {
            Err((StatusCode::NOT_FOUND, "No such endpoint registered!"))
        }
    } else if let Ok(eid) = EndpointID::try_from(path) {
        if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&eid) {
            if let Some(mut bundle) = aa.pop() {
                let cbor_bundle = bundle.to_cbor();
                Ok(cbor_bundle)
            } else {
                Ok("Nothing to receive".as_bytes().to_vec())
            }
        } else {
            Err((StatusCode::NOT_FOUND, "No such endpoint registered!"))
        }
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            "Malformed endpoint path, only alphanumeric strings allowed!",
        ))
    }
}

//#[get("/endpoint.hex", guard = "fn_guard_localhost")]
async fn endpoint_hex(
    extract::RawQuery(query): extract::RawQuery,
) -> Result<String, (StatusCode, &'static str)> {
    if query.is_none() {
        return Err((StatusCode::BAD_REQUEST, "missing query parameter"));
    }
    let path = query.unwrap();
    if path.chars().all(char::is_alphanumeric) {
        let host_eid = (*CONFIG.lock()).host_eid.clone();
        let eid = host_eid
            .new_endpoint(&path)
            .expect("Error constructing new endpoint");
        // TODO: support non-node-specific EIDs
        if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&eid) {
            if let Some(mut bundle) = aa.pop() {
                Ok(bp7::helpers::hexify(&bundle.to_cbor()))
            } else {
                Ok("Nothing to receive".to_string())
            }
        } else {
            Err((StatusCode::NOT_FOUND, "No such endpoint registered!"))
        }
    } else if let Ok(eid) = EndpointID::try_from(path) {
        if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&eid) {
            if let Some(mut bundle) = aa.pop() {
                Ok(bp7::helpers::hexify(&bundle.to_cbor()))
            } else {
                Ok("Nothing to receive".to_string())
            }
        } else {
            Err((StatusCode::NOT_FOUND, "No such endpoint registered!"))
        }
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            "Malformed endpoint path, only alphanumeric strings allowed!",
        ))
    }
}

//#[get("/download")]
async fn download(
    extract::RawQuery(query): extract::RawQuery,
) -> Result<Vec<u8>, (StatusCode, &'static str)> {
    if let Some(bid) = query {
        if let Some(mut bundle) = (*STORE.lock()).get_bundle(&bid) {
            let cbor_bundle = bundle.to_cbor();
            Ok(cbor_bundle)
        } else {
            Err((StatusCode::NOT_FOUND, "Bundle not found"))
        }
    } else {
        Err((StatusCode::BAD_REQUEST, "Bundle ID not specified"))
    }
}

async fn download_hex(
    extract::RawQuery(query): extract::RawQuery,
) -> Result<String, (StatusCode, &'static str)> {
    if let Some(bid) = query {
        if let Some(mut bundle) = (*STORE.lock()).get_bundle(&bid) {
            Ok(bp7::helpers::hexify(&bundle.to_cbor()))
        } else {
            Err((StatusCode::BAD_REQUEST, "Bundle not found"))
        }
    } else {
        Err((http::StatusCode::BAD_REQUEST, "Bundle ID not specified"))
    }
}

pub async fn spawn_httpd() -> Result<()> {
    let app_local_only = Router::new()
        .route("/send", post(send_post))
        .route("/register", get(register))
        .route("/unregister", get(unregister))
        .route("/endpoint", get(endpoint))
        .route("/insert", get(insert_get).post(insert_post))
        .route("/endpoint.hex", get(endpoint_hex))
        .route("/cts", get(get_creation_timestamp))
        .route(
            "/ws",
            get(|ws: WebSocketUpgrade| async move { ws.on_upgrade(super::ws::handle_socket) }),
        )
        .route("/debug/rnd_bundle", get(debug_rnd_bundle))
        .route("/debug/rnd_peer", get(debug_rnd_peer))
        .layer(extractor_middleware::<RequireLocalhost>());
    let app = app_local_only
        .route("/", get(index))
        .route("/peers", get(web_peers))
        .route("/bundles", get(web_bundles))
        .route("/download.hex", get(download_hex))
        .route("/download", get(download))
        .route("/push", post(push_post))
        .route("/status/nodeid", get(status_node_id))
        .route("/status/eids", get(status_eids))
        .route("/status/bundles", get(status_bundles))
        .route("/status/bundles_dest", get(status_bundles_dest))
        .route("/status/store", get(status_store))
        .route("/status/peers", get(status_peers))
        .route("/status/info", get(status_info));

    let port = (*CONFIG.lock()).webport;

    let v4 = (*CONFIG.lock()).v4;
    let v6 = (*CONFIG.lock()).v6;
    //debug!("starting webserver");
    let server = if v4 && !v6 {
        hyper::Server::bind(&format!("0.0.0.0:{}", port).parse()?)
    } else if !v4 && v6 {
        hyper::Server::bind(&format!("[::1]:{}", port).parse()?)
    } else {
        hyper::Server::bind(&format!("[::]:{}", port).parse()?)
    }
    .serve(app.into_make_service_with_connect_info::<SocketAddr, _>());
    server.await?;
    Ok(())
}
