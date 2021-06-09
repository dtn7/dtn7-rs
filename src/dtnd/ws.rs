use crate::CONFIG;
use crate::DTNCORE;
use actix::*;
use actix_web_actors::ws;

use anyhow::anyhow;
use bp7::dtntime::CreationTimestamp;
use bp7::EndpointID;
use dtn7_plus::client::WsRecvData;
use dtn7_plus::client::WsSendData;
use futures::StreamExt;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::{
    convert::{TryFrom, TryInto},
    time::{Duration, Instant},
};

// Begin application agent WebSocket specific stuff

/// How often new bundles are checked
const CHECK_INTERVAL: Duration = Duration::from_millis(20);
/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// WebSocket Applicatin Agent Session
pub(crate) struct WsAASession {
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
impl WsAASession {
    pub fn new() -> WsAASession {
        WsAASession {
            id: 0,
            hb: Instant::now(),
            endpoints: None,
            mode: WsReceiveMode::Data,
        }
    }
}

pub(crate) enum WsReceiveMode {
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
                            let rt = tokio::runtime::Handle::current();
                            rt.spawn(
                                async move { crate::core::processing::send_bundle(bndl).await },
                            );
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
                            let dst = EndpointID::try_from(send_req.dst);
                            let src = EndpointID::try_from(send_req.src);
                            if dst.is_err() || src.is_err() {
                                warn!(
                                    "Received data with invalid src ({}) or destination ({})",
                                    send_req.src, send_req.dst
                                );
                                return;
                            }
                            let src2 = src.unwrap();
                            let pblock = bp7::primary::PrimaryBlockBuilder::default()
                                .bundle_control_flags(bcf)
                                .destination(dst.unwrap())
                                .source(src2.clone())
                                .report_to(src2)
                                .creation_timestamp(CreationTimestamp::now())
                                .lifetime(Duration::from_millis(send_req.lifetime))
                                .build()
                                .unwrap();

                            let b_len = send_req.data.len();
                            debug!("Received via WS for sending: {:?} bytes", b_len);
                            let mut bndl = bp7::bundle::BundleBuilder::default()
                                .primary(pblock)
                                .canonicals(vec![
                                    bp7::canonical::new_payload_block(0, send_req.data.to_owned()),
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
                            let rt = tokio::runtime::Handle::current();
                            rt.spawn(
                                async move { crate::core::processing::send_bundle(bndl).await },
                            );
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
                            let recv_data = match act.mode {
                                WsReceiveMode::Bundle => bundle.to_cbor(),
                                WsReceiveMode::Data => {
                                    if bundle.payload().is_none() {
                                        // No payload -> nothing to deliver to client
                                        // In bundle mode delivery happens because custom canoncial bocks could be present
                                        continue;
                                    }
                                    let recv = WsRecvData {
                                        bid: &bundle.id(),
                                        src: &bundle.primary.source.to_string(),
                                        dst: &bundle.primary.destination.to_string(),
                                        data: &bundle.payload().unwrap(),
                                    };
                                    serde_cbor::to_vec(&recv)
                                        .expect("Fatal error encoding WsRecvData")
                                }
                            };
                            ctx.binary(recv_data);
                        }
                    }
                }
            }
        });
    }
}
