use crate::core::application_agent::SimpleApplicationAgent;
use crate::core::helpers::rnd_peer;
use crate::core::{bundlepack::BundlePack, peer::PeerType};
use crate::peers_count;
use crate::DtnConfig;
use crate::CONFIG;
use crate::DTNCORE;
use crate::PEERS;
use crate::STATS;
use crate::STORE;
use actix::*;
use actix_web::dev::RequestHead;
use actix_web::HttpResponse;
use actix_web::{
    get, http::StatusCode, post, web, App, Error, HttpRequest, HttpServer, Responder, Result,
};
use actix_web_actors::ws;
use dtn7_plus::client::WsSendData;
use std::collections::HashSet;

use anyhow::anyhow;
use bp7::dtntime::CreationTimestamp;
use bp7::helpers::rnd_bundle;
use bp7::EndpointID;
use futures::StreamExt;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::{
    convert::{TryFrom, TryInto},
    time::{Duration, Instant},
};
use tinytemplate::TinyTemplate;

// Begin application agent WebSocket specific stuff

/// How often new bundles are checked
const CHECK_INTERVAL: Duration = Duration::from_millis(50);
/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// WebSocket Applicatin Agent Session
struct WsAASession {
    /// unique session id
    id: usize,
    /// Client must send ping at least once per 10 seconds (CLIENT_TIMEOUT),
    /// otherwise we drop connection.
    hb: Instant,
    /// list of endpoints subscribed to
    endpoints: Option<HashSet<EndpointID>>,
    /// receive either complete bundles or data and construct bundle server side
    mode: WsReceiveMode,
}

enum WsReceiveMode {
    Bundle,
    Data,
}

impl Actor for WsAASession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // we'll start heartbeat process on session start.
        self.hb(ctx);
        self.monitor(ctx);
        debug!("Started new WebSocket for application agent");
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        debug!("Stopped WebSocket for application agent");
        Running::Stop
    }
}
/// WebSocket message handler
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsAASession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Err(_) => {
                ctx.stop();
                return;
            }
            Ok(msg) => msg,
        };

        debug!("WEBSOCKET MESSAGE: {:?}", msg);
        match msg {
            ws::Message::Ping(msg) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            ws::Message::Pong(_) => {
                self.hb = Instant::now();
            }
            ws::Message::Text(text) => {
                let m = text.trim();
                if m.starts_with('/') {
                    let v: Vec<&str> = m.splitn(2, ' ').collect();
                    match v[0] {
                        "/bundle" => {
                            self.mode = WsReceiveMode::Bundle;
                            ctx.text("200 tx mode: bundle");
                        }
                        "/data" => {
                            self.mode = WsReceiveMode::Data;
                            ctx.text("200 tx mode: data");
                        }
                        "/unsubscribe" => {
                            if v.len() == 2 {
                                if let Ok(eid) = EndpointID::try_from(v[1]) {
                                    if let Some(endpoints) = &mut self.endpoints {
                                        endpoints.remove(&eid);
                                        debug!("unsubscribed endpoint: {}", eid);
                                        ctx.text("200 unsubscribed");
                                    } else {
                                        ctx.text("404 endpoint not found");
                                    }
                                } else {
                                    ctx.text("400 invalid endpoint");
                                }
                            }
                        }
                        "/subscribe" => {
                            if v.len() == 2 {
                                if let Ok(eid) = EndpointID::try_from(v[1]) {
                                    if (*DTNCORE.lock()).is_in_endpoints(&eid) {
                                        debug!("subscribed to endpoint: {}", eid);
                                        if self.endpoints.is_none() {
                                            self.endpoints = Some(HashSet::new());
                                        }
                                        if let Some(endpoints) = &mut self.endpoints {
                                            endpoints.insert(eid);
                                        }
                                        ctx.text("200 subscribed");
                                    } else {
                                        debug!(
                                            "Attempted to subscribe to unknown endpoint: {}",
                                            eid
                                        );
                                        ctx.text("404 unknown endpoint");
                                    }
                                } else {
                                    let this_host: EndpointID = (*CONFIG.lock()).host_eid.clone();
                                    if let Ok(eid) = this_host.new_endpoint(v[1]) {
                                        if (*DTNCORE.lock()).get_endpoint(&eid).is_none() {
                                            debug!(
                                                "Attempted to subscribe to unknown endpoint: {}",
                                                eid
                                            );
                                            ctx.text("404 unknown endpoint");
                                        } else {
                                            debug!("Subscribed to endpoint: {}", eid);
                                            if self.endpoints.is_none() {
                                                self.endpoints = Some(HashSet::new());
                                            }
                                            if let Some(endpoints) = &mut self.endpoints {
                                                endpoints.insert(eid);
                                            }
                                            ctx.text("200 subscribed");
                                        }
                                    } else {
                                        debug!(
                                            "Invalid endpoint combination: {} and {}",
                                            this_host, v[1]
                                        );
                                        ctx.text("400 invalid endpoint combination");
                                    }
                                }
                            } else {
                                ctx.text("400 endpoint is missing");
                            }
                        }
                        _ => ctx.text(format!("501 unknown command: {:?}", m)),
                    }
                } else {
                }
            }
            ws::Message::Binary(bin) => {
                match self.mode {
                    WsReceiveMode::Bundle => {
                        if let Ok(bndl) = serde_cbor::from_slice::<bp7::Bundle>(&bin) {
                            debug!(
                                "Sending bundle {} to {} from WS",
                                bndl.id(),
                                bndl.primary.destination
                            );
                            // TODO: turn into channel
                            //                            crate::core::processing::send_bundle(bndl);
                            //crate::core::processing::send_through_task(bndl);
                            /*let rt = tokio::runtime::Handle::current();
                            rt.spawn(
                                async move { crate::core::processing::send_bundle(bndl).await },
                            );*/

                            let mut rt = actix_rt::Runtime::new().unwrap();
                            let send = crate::core::processing::send_through_task_async(bndl);
                            //rt.block_on(send);
                            //actix_rt::Arbiter::spawn(send);
                            rt.spawn(send);

                            ctx.text(format!("200 Sent payload with {} bytes", bin.len()));
                        } else {
                            ctx.text("400 Invalid binary bundle");
                        }
                    }
                    WsReceiveMode::Data => {
                        if let Ok(send_req) = serde_cbor::from_slice::<WsSendData>(&bin) {
                            //let src = (*CONFIG.lock()).host_eid.clone();
                            let bcf = if send_req.delivery_notification {
                                bp7::bundle::BUNDLE_MUST_NOT_FRAGMENTED
                                    | bp7::bundle::BUNDLE_STATUS_REQUEST_DELIVERY
                            } else {
                                bp7::bundle::BUNDLE_MUST_NOT_FRAGMENTED
                            };
                            let pblock = bp7::primary::PrimaryBlockBuilder::default()
                                .bundle_control_flags(bcf)
                                .destination(send_req.dst)
                                .source(send_req.src.clone())
                                .report_to(send_req.src)
                                .creation_timestamp(CreationTimestamp::now())
                                .lifetime(send_req.lifetime)
                                .build()
                                .unwrap();

                            let b_len = send_req.data.len();
                            debug!("Received via WS for sending: {:?} bytes", b_len);
                            let mut bndl = bp7::bundle::BundleBuilder::default()
                                .primary(pblock)
                                .canonicals(vec![
                                    bp7::canonical::new_payload_block(0, send_req.data),
                                    bp7::canonical::new_hop_count_block(2, 0, 32),
                                ])
                                .build()
                                .unwrap();
                            bndl.set_crc(bp7::crc::CRC_NO);

                            debug!(
                                "Sending bundle {} to {} from WS",
                                bndl.id(),
                                bndl.primary.destination
                            );
                            //let mut rt = tokio::runtime::Runtime::new().unwrap();
                            //rt.block_on(async { crate::core::processing::send_bundle(bndl).await });
                            /*let rt = tokio::runtime::Handle::current();
                            rt.spawn(
                                async move { crate::core::processing::send_bundle(bndl).await },
                            );*/
                            let mut rt = actix_rt::Runtime::new().unwrap();
                            let send = crate::core::processing::send_through_task_async(bndl);
                            //rt.block_on(send);
                            //actix_rt::Arbiter::spawn(send);
                            rt.spawn(send);

                            //crate::core::processing::send_through_task(bndl);
                            ctx.text(format!("200 Sent payload with {} bytes", b_len));
                        } else {
                            ctx.text("400 Unexpected binary");
                        }
                    }
                }
            }
            ws::Message::Close(_) => {
                ctx.stop();
            }
            ws::Message::Continuation(_) => {
                ctx.stop();
            }
            ws::Message::Nop => (),
        }
    }
}
impl WsAASession {
    /// helper method that sends ping to client every second.
    ///
    /// also this method checks heartbeats from client
    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                // heartbeat timed out
                debug!("Websocket Client heartbeat failed, disconnecting!");

                // stop actor
                ctx.stop();

                // don't try to send a ping
                return;
            }

            ctx.ping(b"");
        });
    }
    /// helper method that checks for new bundles.
    fn monitor(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(CHECK_INTERVAL, |act, ctx| {
            if let Some(endpoints) = act.endpoints.clone() {
                for eid in endpoints {
                    if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&eid) {
                        if let Some(mut bundle) = aa.pop() {
                            let cbor_bundle = bundle.to_cbor();
                            ctx.binary(cbor_bundle);
                        }
                    }
                }
            }
        });
    }
}

#[get("/ws", guard = "fn_guard_localhost")]
async fn ws_application_agent(
    req: HttpRequest,
    stream: web::Payload,
) -> Result<HttpResponse, Error> {
    ws::start(
        WsAASession {
            id: 0,
            hb: Instant::now(),
            endpoints: None,
            mode: WsReceiveMode::Data,
        },
        &req,
        stream,
    )
}

// End application agent WebSocket specific stuff

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

pub fn fn_guard_localhost(req: &RequestHead) -> bool {
    if (*CONFIG.lock()).unsafe_httpd {
        return true;
    }
    if let Some(addr) = req.peer_addr {
        if addr.ip().is_loopback() {
            return true;
        } else {
            if let std::net::IpAddr::V6(ipv6) = addr.ip() {
                if let Some(ipv4) = ipv6.to_ipv4() {
                    return ipv4.is_loopback();
                }
            }
        }
    }
    false
}

#[get("/")]
async fn index() -> impl Responder {
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
        num_bundles: (*DTNCORE.lock()).bundles().len(),
    };

    let rendered = tt
        .render("index", &context)
        .expect("error rendering template");
    HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(rendered)
}

#[get("/peers")]
async fn web_peers() -> impl Responder {
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
                con_type: p.con_type.clone(),
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
    HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(rendered)
}

use humansize::{file_size_opts, FileSize};
#[get("/bundles")]
async fn web_bundles() -> impl Responder {
    // "dtn7 ctrl interface"
    let template_str = include_str!("../../webroot/bundles.html");
    let mut tt = TinyTemplate::new();
    tt.add_template("bundles", template_str)
        .expect("error adding template");
    let bundles_vec: Vec<BundleInfo> = (STORE.lock())
        .bundles()
        .iter()
        .map(|bp| BundleInfo {
            id: bp.id.to_string(),
            size: bp.size.file_size(file_size_opts::DECIMAL).unwrap().into(),
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
    HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(rendered)
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
#[get("/status/store", guard = "fn_guard_localhost")]
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

#[get("/cts", guard = "fn_guard_localhost")]
async fn creation_timestamp() -> String {
    let cts = bp7::CreationTimestamp::now();
    serde_json::to_string(&cts).unwrap()
}

#[get("/debug/rnd_bundle", guard = "fn_guard_localhost")]
async fn debug_rnd_bundle() -> String {
    println!("generating debug bundle");
    let bndl = rnd_bundle(CreationTimestamp::now());
    let res = bndl.id();
    //crate::core::processing::send_bundle(bndl).await;
    crate::core::processing::send_through_task_async(bndl).await;
    res
}

#[get("/debug/rnd_peer", guard = "fn_guard_localhost")]
async fn debug_rnd_peer() -> String {
    println!("generating debug peer");
    let p = rnd_peer();
    let res = serde_json::to_string_pretty(&p).unwrap();
    (*PEERS.lock()).insert(p.eid.node().unwrap_or_default(), p);
    res
}
#[get("/insert", guard = "fn_guard_localhost")]
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

                //crate::core::processing::send_bundle(bndl).await;
                crate::core::processing::send_through_task_async(bndl).await;
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
#[post("/insert", guard = "fn_guard_localhost")]
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

        //crate::core::processing::send_bundle(bndl).await;
        crate::core::processing::send_through_task_async(bndl).await;
        //crate::core::processing::send_through_task(bndl);
        Ok(format!("Sent {} bytes", b_len))
    } else {
        Err(actix_web::error::ErrorBadRequest(anyhow!(
            "Error decoding bundle!"
        )))
    }
}

#[post("/send", guard = "fn_guard_localhost")]
async fn send_post(req: HttpRequest, mut body: web::Payload) -> Result<String> {
    let params = url::form_urlencoded::parse(req.query_string().as_bytes());
    let mut dst: EndpointID = EndpointID::none();
    let mut lifetime = std::time::Duration::from_secs(60 * 60);
    for (k, v) in params {
        if k == "dst" {
            dst = v.to_string().try_into().unwrap();
        } else if k == "lifetime" {
            if let Ok(dur) = humantime::parse_duration(&v) {
                lifetime = dur;
            }
        }
    }
    if dst == EndpointID::none() {
        return Err(actix_web::error::ErrorBadRequest(anyhow!(
            "Missing destination endpoint id!"
        )));
    }
    let src = (*CONFIG.lock()).host_eid.clone();
    let pblock = bp7::primary::PrimaryBlockBuilder::default()
        .bundle_control_flags(
            bp7::bundle::BUNDLE_MUST_NOT_FRAGMENTED | bp7::bundle::BUNDLE_STATUS_REQUEST_DELIVERY,
        )
        .destination(dst)
        .source(src.clone())
        .report_to(src)
        .creation_timestamp(CreationTimestamp::now())
        .lifetime(lifetime)
        .build()
        .unwrap();

    let mut bytes = web::BytesMut::new();
    while let Some(item) = body.next().await {
        bytes.extend_from_slice(&item?);
    }
    let b_len = bytes.len();
    debug!("Received for sending: {:?}", b_len);
    let mut bndl = bp7::bundle::BundleBuilder::default()
        .primary(pblock)
        .canonicals(vec![
            bp7::canonical::new_payload_block(0, bytes.to_vec()),
            bp7::canonical::new_hop_count_block(2, 0, 32),
        ])
        .build()
        .unwrap();
    bndl.set_crc(bp7::crc::CRC_NO);

    debug!(
        "Sending bundle {} to {}",
        bndl.id(),
        bndl.primary.destination
    );

    //crate::core::processing::send_bundle(bndl).await;
    crate::core::processing::send_through_task_async(bndl).await;
    Ok(format!("Sent payload with {} bytes", b_len))
}

#[post("/push")]
async fn push_post(mut body: web::Payload) -> Result<String> {
    let mut bytes = web::BytesMut::new();
    while let Some(item) = body.next().await {
        bytes.extend_from_slice(&item?);
    }
    let b_len = bytes.len();
    debug!("Received: {:?}", b_len);
    if let Ok(bndl) = bp7::Bundle::try_from(bytes.to_vec()) {
        info!("Received bundle {}", bndl.id());
        crate::core::processing::receive(bndl.into()).await;
        Ok(format!("Received {} bytes", b_len))
    } else {
        Err(actix_web::error::ErrorBadRequest(anyhow!(
            "Error decoding bundle!"
        )))
    }
}

#[get("/register", guard = "fn_guard_localhost")]
async fn register(req: HttpRequest) -> Result<String> {
    let path = req.query_string();
    // TODO: support non-node-specific EIDs
    if path.chars().all(char::is_alphanumeric) {
        let host_eid = (*CONFIG.lock()).host_eid.clone();
        let eid = host_eid
            .new_endpoint(path)
            .expect("Error constructing new endpoint");
        (*DTNCORE.lock()).register_application_agent(SimpleApplicationAgent::with(eid.clone()));
        Ok(format!("Registered {}", eid))
    } else {
        if let Ok(eid) = EndpointID::try_from(path) {
            (*DTNCORE.lock()).register_application_agent(SimpleApplicationAgent::with(eid.clone()));
            Ok(format!("Registered URI: {}", eid))
        } else {
            Err(actix_web::error::ErrorBadRequest(anyhow!(
                "Malformed endpoint path, only alphanumeric strings or endpoint URIs are allowed!"
            )))
        }
    }
}

#[get("/unregister", guard = "fn_guard_localhost")]
async fn unregister(req: HttpRequest) -> Result<String> {
    let path = req.query_string();
    if path.chars().all(char::is_alphanumeric) {
        let host_eid = (*CONFIG.lock()).host_eid.clone();
        let eid = host_eid
            .new_endpoint(path)
            .expect("Error constructing new endpoint");

        (*DTNCORE.lock()).unregister_application_agent(SimpleApplicationAgent::with(eid.clone()));
        Ok(format!("Unregistered {}", eid))
    } else {
        if let Ok(eid) = EndpointID::try_from(path) {
            (*DTNCORE.lock())
                .unregister_application_agent(SimpleApplicationAgent::with(eid.clone()));
            Ok(format!("Unregistered URI: {}", eid))
        } else {
            Err(actix_web::error::ErrorBadRequest(anyhow!(
                "Malformed endpoint path, only alphanumeric strings or endpoint URIs are allowed!"
            )))
        }
    }
}

#[get("/endpoint", guard = "fn_guard_localhost")]
async fn endpoint(req: HttpRequest) -> Result<HttpResponse> {
    let path = req.query_string();
    if path.chars().all(char::is_alphanumeric) {
        let host_eid = (*CONFIG.lock()).host_eid.clone();
        let eid = host_eid
            .new_endpoint(path)
            .expect("Error constructing new endpoint"); // TODO: support non-node-specific EIDs
        if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&eid) {
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
        if let Ok(eid) = EndpointID::try_from(path) {
            if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&eid) {
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
}
#[get("/endpoint.hex", guard = "fn_guard_localhost")]
async fn endpoint_hex(req: HttpRequest) -> Result<String> {
    let path = req.query_string();
    if path.chars().all(char::is_alphanumeric) {
        let host_eid = (*CONFIG.lock()).host_eid.clone();
        let eid = host_eid
            .new_endpoint(path)
            .expect("Error constructing new endpoint");
        // TODO: support non-node-specific EIDs
        if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&eid) {
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
        if let Ok(eid) = EndpointID::try_from(path) {
            if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&eid) {
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
    //let local = tokio::task::LocalSet::new();
    //let sys = actix_web::rt::System::run_in_tokio("server", &local);

    //let sys = actix_web::rt::System::new("http_server");
    let server = HttpServer::new(|| {
        App::new()
            .service(index)
            .service(web_peers)
            .service(web_bundles)
            .service(status_node_id)
            .service(status_eids)
            .service(status_bundles)
            .service(status_bundles_dest)
            .service(status_store)
            .service(status_peers)
            .service(status_info)
            .service(creation_timestamp)
            .service(debug_rnd_bundle)
            .service(debug_rnd_peer)
            .service(insert_get)
            .service(insert_post)
            .service(send_post)
            .service(push_post)
            .service(register)
            .service(unregister)
            .service(endpoint)
            .service(endpoint_hex)
            .service(download)
            .service(download_hex)
            .service(ws_application_agent)
    });
    let v4 = (*CONFIG.lock()).v4;
    let v6 = (*CONFIG.lock()).v6;
    let server = if v4 && !v6 {
        server.bind(&format!("0.0.0.0:{}", port))?
    } else if !v4 && v6 {
        server.bind(&format!("[::1]:{}", port))?
    } else {
        server.bind(&format!("[::]:{}", port))?
    };
    server.run().await
    //sys.await
}
