use crate::core::application_agent::ApplicationAgent;
use crate::core::application_agent::SimpleApplicationAgent;
use crate::core::bundlepack::Constraint;
use crate::core::helpers::get_complete_digest;
use crate::core::helpers::get_digest_of_bids;
use crate::core::helpers::is_valid_service_name;
use crate::core::helpers::rnd_peer;
use crate::core::peer::PeerType;
use crate::core::store::BundleStore;
use crate::peers_add;
use crate::peers_remove;
use crate::routing_cmd;
use crate::routing_get_data;
use crate::store_remove;
use crate::CONFIG;
use crate::DTNCORE;
use crate::PEERS;
use crate::STATS;
use crate::STORE;
use crate::{cla_names, peers_count};
use crate::{DtnConfig, PeerAddress};
use anyhow::Result;
use async_trait::async_trait;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::DefaultBodyLimit;
use axum::extract::Query;
use axum::response::Html;
use axum::{
    extract::{self, connect_info::ConnectInfo, RequestParts},
    middleware::from_extractor,
    routing::{get, post},
    Router,
};
use bp7::dtntime::CreationTimestamp;
use bp7::flags::BlockControlFlags;
use bp7::flags::BundleControlFlags;
use bp7::helpers::rnd_bundle;
use bp7::EndpointID;
use http::StatusCode;
use humansize::format_size;
use humansize::DECIMAL;
use log::info;
use log::trace;
use log::{debug, warn};
use serde::Serialize;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::fmt::Write;
use std::net::SocketAddr;
use std::time::Instant;
use tinytemplate::TinyTemplate;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;
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
        if CONFIG.lock().unsafe_httpd {
            return Ok(Self);
        }
        if let Some(ConnectInfo(addr)) = conn.extensions().get::<ConnectInfo<SocketAddr>>() {
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
    bundles_digest: String,
    clas: Vec<String>,
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
    addr: PeerAddress,
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
    bundles_digest: String,
}

// End of web UI specific structs

//#[get("/")]
async fn index() -> Html<String> {
    // "dtn7 ctrl interface"
    let template_str = include_str!("../../webroot/index.html");
    let mut tt = TinyTemplate::new();
    tt.add_template("index", template_str)
        .expect("error adding template");
    let announcement = humantime::format_duration(CONFIG.lock().announcement_interval).to_string();
    let janitor = humantime::format_duration(CONFIG.lock().janitor_interval).to_string();
    let timeout = humantime::format_duration(CONFIG.lock().peer_timeout).to_string();
    let bundles_digest = get_complete_digest();
    let clas = cla_names();
    let context = IndexContext {
        config: &CONFIG.lock(),
        announcement,
        janitor,
        timeout,
        num_peers: peers_count(),
        num_bundles: (*DTNCORE.lock()).bundle_count(),
        bundles_digest,
        clas,
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
    tt.add_formatter(
        "dump_json",
        |value: &serde_json::Value,
         output: &mut String|
         -> Result<(), tinytemplate::error::Error> {
            write!(output, "{}", value)?;
            Ok(())
        },
    );
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
                addr: p.addr.clone(),
                last: time_since,
            }
        })
        .collect();

    let context = PeersContext {
        config: &CONFIG.lock(),
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
            size: format_size(bp.size, DECIMAL),
        })
        .collect();
    let bundles_digest = get_complete_digest();
    let context = BundlesContext {
        config: &CONFIG.lock(),
        bundles: bundles_vec.as_slice(),
        bundles_digest,
    };
    //let peers_vec: Vec<&DtnPeer> = (*PEERS.lock()).values().collect();
    let rendered = tt
        .render("bundles", &context)
        .expect("error rendering template");
    Html(rendered)
}

//#[get("/status/nodeid")]
async fn status_node_id() -> String {
    CONFIG.lock().host_eid.to_string()
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
async fn status_bundles_filtered(
    Query(params): Query<HashMap<String, String>>,
) -> Result<String, (StatusCode, &'static str)> {
    if let Some(criteria) = params.get("addr") {
        let bids = (*STORE.lock()).filter_addr(criteria);
        Ok(serde_json::to_string_pretty(&bids).unwrap())
    } else {
        //anyhow::bail!("missing filter criteria");
        Err((StatusCode::BAD_REQUEST, "missing filter criteria"))
    }
}
async fn status_bundles_filtered_digest(
    Query(params): Query<HashMap<String, String>>,
) -> Result<String, (StatusCode, &'static str)> {
    if let Some(criteria) = params.get("addr") {
        let bids = (*STORE.lock()).filter_addr(criteria);
        Ok(get_digest_of_bids(&bids))
    } else {
        //anyhow::bail!("missing filter criteria");
        Err((StatusCode::BAD_REQUEST, "missing filter criteria"))
    }
}
//#[get("/status/bundles/verbose")]
async fn status_bundles_verbose() -> String {
    serde_json::to_string_pretty(&(*DTNCORE.lock()).bundle_full_meta()).unwrap()
}
//#[get("/status/bundles/digest")]
async fn status_bundles_digest() -> String {
    get_complete_digest()
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
    STATS.lock().update_node_stats();
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

async fn http_routing_cmd(
    Query(params): Query<HashMap<String, String>>,
) -> Result<String, (StatusCode, &'static str)> {
    if let Some(cmd) = params.get("c") {
        if routing_cmd(cmd.to_string()).await.is_ok() {
            Ok("Sent command to routing agent.".into())
        } else {
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "error sending cmd to routing agent",
            ))
        }
    } else {
        //anyhow::bail!("missing filter criteria");
        Err((
            StatusCode::BAD_REQUEST,
            "missing routing command parameter cmd",
        ))
    }
}

async fn http_routing_getdata(
    Query(params): Query<HashMap<String, String>>,
) -> Result<String, (StatusCode, &'static str)> {
    let param = params.get("p").map_or("".to_string(), |f| f.to_string());
    if let Ok(res) = routing_get_data(param).await {
        Ok(res)
    } else {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "error getting data from routing agent",
        ))
    }
}

async fn http_peers_add(
    Query(params): Query<HashMap<String, String>>,
) -> Result<String, (StatusCode, &'static str)> {
    if let Some(peer_str) = params.get("p") {
        let peer_type =
            PeerType::try_from(params.get("p_t").unwrap_or(&"DYNAMIC".to_owned()).as_str())
                .unwrap();
        let mut peer = if let Ok(parsed_peer) = crate::core::helpers::parse_peer_url(peer_str) {
            parsed_peer
        } else {
            return Err((StatusCode::BAD_REQUEST, "Malformed peer URL"));
        };
        peer.con_type = peer_type;

        let is_new = peers_add(peer);
        if is_new {
            Ok("Added new peer".into())
        } else {
            Ok("Updated existing peer".into())
        }
    } else {
        //anyhow::bail!("missing filter criteria");
        Err((StatusCode::BAD_REQUEST, "missing peer parameter p"))
    }
}
async fn http_peers_delete(
    Query(params): Query<HashMap<String, String>>,
) -> Result<String, (StatusCode, &'static str)> {
    if let Some(peer_str) = params.get("p") {
        // TODO: make it return a result
        let peer = if let Ok(parsed_peer) = crate::core::helpers::parse_peer_url(peer_str) {
            parsed_peer
        } else {
            return Err((StatusCode::BAD_REQUEST, "Malformed peer URL"));
        };

        // TODO: test with IPN
        peers_remove(&peer.eid.node().unwrap());

        Ok("Removed peer".into())
    } else {
        //anyhow::bail!("missing filter criteria");
        Err((StatusCode::BAD_REQUEST, "missing peer parameter p"))
    }
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
                if bndl.primary.source.node() == (*CONFIG.lock()).host_eid.node() {
                    STATS.lock().node.bundles.bundles_created += 1;
                }
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

        if bndl.primary.source.node() == (*CONFIG.lock()).host_eid.node() {
            STATS.lock().node.bundles.bundles_created += 1;
        }

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
    let mut flags = BundleControlFlags::BUNDLE_MUST_NOT_FRAGMENTED;
    //    | BundleControlFlags::BUNDLE_STATUS_REQUEST_DELIVERY;

    for (k, v) in query_params.iter() {
        if k == "dst" {
            dst = v.as_str().try_into().unwrap();
        } else if k == "lifetime" {
            if let Ok(dur) = humantime::parse_duration(v) {
                lifetime = dur;
            }
        } else if k == "flags" {
            let param_flags: u64 = v.as_str().parse().unwrap_or(0);
            if let Some(bpcf) = BundleControlFlags::from_bits(param_flags) {
                flags = bpcf;
            } else {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Invalid Bundle Processing Control Flags!",
                ));
            }
        }
    }
    if dst == EndpointID::none() {
        return Err((StatusCode::BAD_REQUEST, "Missing destination endpoint id!"));
    }
    let src = CONFIG.lock().host_eid.clone();

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
        &bndl.id(),
        bndl.primary.destination
    );
    let bid = bndl.id();
    crate::core::processing::send_bundle(bndl).await;
    STATS.lock().node.bundles.bundles_created += 1;
    Ok(format!("Sent ADU in bundle {} with {} bytes", bid, b_len))
}

//#[post("/push")]
async fn push_post(body: bytes::Bytes) -> Result<String, (StatusCode, String)> {
    let b_len = body.len();
    trace!("received via /push: {:?} bytes", b_len);
    if let Ok(bndl) = bp7::Bundle::try_from(body.as_ref()) {
        //trace!("received bundle {}", bndl.id());
        info!("Received bundle: {}", bndl.id());
        let bid = bndl.id();
        //tokio::spawn(async move {
        let now = Instant::now();
        if let Err(err) = crate::core::processing::receive(bndl).await {
            warn!("Error processing bundle: {}", err);
        }
        let elapsed = now.elapsed();
        debug!("Processed received bundle {} in {:?}", bid, elapsed);
        //});
        Ok(format!("Received {} bytes", b_len))
    } else {
        crate::STATS.lock().broken += 1;
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
    if is_valid_service_name(&path) {
        // without url scheme assume a local DTN service name
        let host_eid = CONFIG.lock().host_eid.clone();
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
    if is_valid_service_name(&path) {
        let host_eid = CONFIG.lock().host_eid.clone();
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
    if is_valid_service_name(&path) {
        let host_eid = CONFIG.lock().host_eid.clone();
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
    if is_valid_service_name(&path) {
        let host_eid = CONFIG.lock().host_eid.clone();
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

//#[get("/delete", guard = "fn_guard_localhost")]
async fn delete(
    extract::RawQuery(query): extract::RawQuery,
) -> Result<Vec<u8>, (StatusCode, &'static str)> {
    if let Some(bid) = query {
        info!("Requested deleting of bundle {}", bid);
        if store_remove(&bid).is_ok() {
            Ok(format!("Deleted {}", bid).as_bytes().to_vec())
        } else {
            Err((StatusCode::NOT_FOUND, "Bundle not found"))
        }
    } else {
        Err((StatusCode::BAD_REQUEST, "Bundle ID not specified"))
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
    let cors = CorsLayer::new()
        // allow `GET` and `POST` when accessing the resource
        .allow_methods([http::Method::GET, http::Method::POST, http::Method::DELETE])
        // allow requests from any origin
        .allow_origin(Any);

    let mut app_local_only = Router::new()
        .route("/peers/add", get(http_peers_add))
        .route("/peers/del", get(http_peers_delete))
        .route("/routing/cmd", get(http_routing_cmd).post(http_routing_cmd))
        .route("/routing/getdata", get(http_routing_getdata))
        .route("/send", post(send_post))
        .layer(DefaultBodyLimit::disable())
        .route("/delete", get(delete).delete(delete))
        .route("/register", get(register))
        .route("/unregister", get(unregister))
        .route("/endpoint", get(endpoint))
        .route("/insert", get(insert_get).post(insert_post))
        .layer(DefaultBodyLimit::disable())
        .route("/endpoint.hex", get(endpoint_hex))
        .route("/cts", get(get_creation_timestamp))
        .route(
            "/ws",
            get(|ws: WebSocketUpgrade| async move {
                ws.max_message_size(128 * 1024 * 1024)
                    .max_frame_size(128 * 1024 * 1024)
                    .on_upgrade(super::ws::handle_socket)
            }),
        )
        .route("/debug/rnd_bundle", get(debug_rnd_bundle))
        .route("/debug/rnd_peer", get(debug_rnd_peer))
        .layer(from_extractor::<RequireLocalhost>())
        .layer(cors.clone());

    if CONFIG.lock().routing == "external" {
        app_local_only = app_local_only.route(
            "/ws/erouting",
            get(|ws: WebSocketUpgrade| async move {
                ws.on_upgrade(crate::routing::erouting::processing::handle_connection)
            }),
        )
    }

    if CONFIG.lock().ecla_enable {
        app_local_only = app_local_only.route(
            "/ws/ecla",
            get(|ws: WebSocketUpgrade| async move {
                ws.on_upgrade(crate::cla::ecla::ws::handle_connection)
            }),
        )
    }

    let app = app_local_only
        .route("/", get(index))
        .route("/peers", get(web_peers))
        .route("/bundles", get(web_bundles))
        .route("/download.hex", get(download_hex))
        .route("/download", get(download))
        .route("/push", post(push_post))
        .layer(DefaultBodyLimit::disable())
        .route("/status/nodeid", get(status_node_id))
        .route("/status/eids", get(status_eids))
        .route("/status/bundles", get(status_bundles))
        .route("/status/bundles/filtered", get(status_bundles_filtered))
        .route(
            "/status/bundles/filtered/digest",
            get(status_bundles_filtered_digest),
        )
        .route("/status/bundles/verbose", get(status_bundles_verbose))
        .route("/status/bundles/digest", get(status_bundles_digest))
        .route("/status/store", get(status_store))
        .route("/status/peers", get(status_peers))
        .route("/status/info", get(status_info))
        .layer(cors.clone());

    let port = CONFIG.lock().webport;

    let v4 = CONFIG.lock().v4;
    let v6 = CONFIG.lock().v6;
    //debug!("starting webserver");
    let server = if v4 && !v6 {
        hyper::Server::bind(&format!("0.0.0.0:{}", port).parse()?)
    } else if !v4 && v6 {
        hyper::Server::bind(&format!("[::1]:{}", port).parse()?)
    } else {
        hyper::Server::bind(&format!("[::]:{}", port).parse()?)
    }
    .serve(app.into_make_service_with_connect_info::<SocketAddr>());
    server.await?;
    Ok(())
}
