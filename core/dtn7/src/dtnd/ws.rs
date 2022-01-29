use crate::core::application_agent::ApplicationAgent;
use crate::CONFIG;
use crate::DTNCORE;

use anyhow::{bail, Result};
use axum::extract::ws::{Message, WebSocket};
use bp7::flags::BlockControlFlags;
use bp7::flags::BundleControlFlags;
use bp7::{Bundle, CreationTimestamp, EndpointID};
use dtn7_plus::client::{WsRecvData, WsSendData};
use futures::{sink::SinkExt, stream::StreamExt};
use log::{debug, warn};
use std::collections::HashSet;
use std::sync::Arc;
use std::{
    convert::TryFrom,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::time::interval;
// Begin application agent WebSocket specific stuff

/// How often new bundles are checked when no direct delivery happens (DEPRECATED)
// const CHECK_INTERVAL: Duration = Duration::from_millis(100);
/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WsReceiveMode {
    Bundle,
    Data(DataReceiveFormat),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DataReceiveFormat {
    CBOR,
    JSON,
}

/// WebSocket Applicatin Agent Session
#[derive(Debug, Clone)]
pub struct WsAASession {
    /// Client must send ping at least once per 10 seconds (CLIENT_TIMEOUT),
    /// otherwise we drop connection.
    hb: Instant,
    /// list of endpoints subscribed to
    endpoints: Option<HashSet<EndpointID>>,
    /// receive either complete bundles or data and construct bundle server side
    mode: WsReceiveMode,
    tx: mpsc::Sender<BundleDelivery>,
}

pub async fn handle_socket(socket: WebSocket) {
    let (session, mut rx_bd) = WsAASession::new();
    let (mut sender, mut receiver) = socket.split();

    let session = Arc::new(Mutex::new(session));
    let (tx, mut rx) = mpsc::channel::<Message>(100);

    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            // In any websocket error, break loop.
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });
    let tx2 = tx.clone();
    let session2 = session.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if session2
                .lock()
                .await
                .handle_message(tx2.clone(), msg)
                .await
                .is_err()
            {
                break;
            }
        }
    });
    let tx2 = tx.clone();
    let session2 = session.clone();
    let mut hb_task = tokio::spawn(async move {
        // wait before sending first heartbeat
        let mut task = interval(CLIENT_TIMEOUT);
        task.tick().await;

        let mut task = interval(HEARTBEAT_INTERVAL);

        loop {
            task.tick().await;
            if Instant::now().duration_since(session2.lock().await.hb) > CLIENT_TIMEOUT {
                // heartbeat timed out
                debug!("Websocket Client heartbeat failed, disconnecting!");

                // don't try to send a ping
                return;
            }

            debug!("sending ping");
            if tx2.send(Message::Ping(b"dtn7".to_vec())).await.is_err() {
                break;
            }
        }
    });

    let tx2 = tx.clone();
    let session2 = session.clone();
    let mut br_task = tokio::spawn(async move {
        while let Some(bndl_delivery) = rx_bd.recv().await {
            debug!("Received bundle delivery for {}", bndl_delivery.0.id());
            if session2
                .lock()
                .await
                .handle_bundle_delivery(tx2.clone(), bndl_delivery)
                .await
                .is_err()
            {
                break;
            }
        }
    });
    // TODO: maybe add legacy fetch_new_bundles call periodically again?
    tokio::select! {
        _ = (&mut send_task) => {recv_task.abort(); hb_task.abort();br_task.abort();},
        _ = (&mut recv_task) => {send_task.abort(); hb_task.abort();br_task.abort();},
        _ = (&mut hb_task) => {send_task.abort(); recv_task.abort(); br_task.abort();},
        _ = (&mut br_task) => {hb_task.abort(); recv_task.abort(); send_task.abort();},
    };

    if let Some(endpoints) = &session.lock().await.endpoints {
        for eid in endpoints {
            if let Some(ep) = (*DTNCORE.lock()).get_endpoint_mut(eid) {
                ep.clear_delivery_addr();
            }
            debug!("connection ended, unsubscribed endpoint: {}", eid);
        }
    };
}

macro_rules! ws_reply_text {
    ($sock:expr,$msg:expr) => {
        if let Err(err) = $sock.send(Message::Text($msg.to_string())).await {
            bail!("err sendin reply: {} -> {}", $msg, err);
        }
    };
}

impl WsAASession {
    pub fn new() -> (WsAASession, mpsc::Receiver<BundleDelivery>) {
        let (tx, rx) = mpsc::channel(32);
        (
            WsAASession {
                hb: Instant::now(),
                endpoints: None,
                mode: WsReceiveMode::Data(DataReceiveFormat::JSON),
                tx,
            },
            rx,
        )
    }
    pub async fn handle_bundle_delivery(
        &self,
        socket: mpsc::Sender<Message>,
        bndl_delivery: BundleDelivery,
    ) -> Result<()> {
        let mut bndl = bndl_delivery.0;
        let recv_data = match self.mode {
            WsReceiveMode::Bundle => bndl.to_cbor(),
            WsReceiveMode::Data(format) => {
                if bndl.payload().is_none() {
                    // No payload -> nothing to deliver to client
                    // In bundle mode delivery happens because custom canoncial bocks could be present
                    return Ok(());
                }
                let recv = WsRecvData {
                    bid: bndl.id(),
                    src: bndl.primary.source.to_string(),
                    dst: bndl.primary.destination.to_string(),
                    data: bndl.payload().unwrap().to_vec(),
                };
                match format {
                    DataReceiveFormat::CBOR => {
                        serde_cbor::to_vec(&recv).expect("Fatal error encoding WsRecvData")
                    }
                    DataReceiveFormat::JSON => {
                        serde_json::to_vec(&recv).expect("Fatal error encoding WsRecvData")
                    }
                }
            }
        };
        if socket.send(Message::Binary(recv_data)).await.is_err() {
            bail!("error sending bundle");
        }
        Ok(())
    }
    pub async fn handle_message(
        &mut self,
        socket: mpsc::Sender<Message>,
        msg: Message,
    ) -> Result<()> {
        debug!("got message: {:?}", msg);

        match msg {
            Message::Text(msg) => {
                let m = msg.trim();
                if m.starts_with('/') {
                    let v: Vec<&str> = m.splitn(2, ' ').collect();
                    match v[0] {
                        "/node" => {
                            ws_reply_text!(
                                socket,
                                &format!("200 node: {}", (*CONFIG.lock()).host_eid)
                            );
                        }
                        "/bundle" => {
                            self.mode = WsReceiveMode::Bundle;
                            ws_reply_text!(socket, "200 tx mode: bundle");
                        }
                        "/data" => {
                            self.mode = WsReceiveMode::Data(DataReceiveFormat::CBOR);

                            ws_reply_text!(socket, "200 tx mode: data");
                        }
                        "/json" => {
                            self.mode = WsReceiveMode::Data(DataReceiveFormat::JSON);

                            ws_reply_text!(socket, "200 tx mode: JSON");
                        }
                        "/unsubscribe" => {
                            if v.len() == 2 {
                                if let Ok(eid) = EndpointID::try_from(v[1]) {
                                    if let Some(endpoints) = &mut self.endpoints {
                                        endpoints.remove(&eid);
                                        if let Some(ep) = (*DTNCORE.lock()).get_endpoint_mut(&eid) {
                                            ep.clear_delivery_addr();
                                        }
                                        debug!("unsubscribed endpoint: {}", eid);

                                        ws_reply_text!(socket, "200 unsubscribed");
                                    } else {
                                        ws_reply_text!(socket, "404 endpoint not found");
                                    }
                                } else {
                                    ws_reply_text!(socket, "400 invalid endpoint");
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
                                        if let Some(ep) = (*DTNCORE.lock()).get_endpoint_mut(&eid) {
                                            ep.set_delivery_addr(self.tx.clone());
                                        }
                                        if let Some(endpoints) = &mut self.endpoints {
                                            endpoints.insert(eid);
                                        }

                                        ws_reply_text!(socket, "200 subscribed");
                                        self.fetch_new_bundles(socket.clone()).await;
                                    } else {
                                        debug!(
                                            "Attempted to subscribe to unknown endpoint: {}",
                                            eid
                                        );

                                        ws_reply_text!(socket, "404 unknown endpoint");
                                    }
                                } else {
                                    let this_host: EndpointID = (*CONFIG.lock()).host_eid.clone();
                                    if let Ok(eid) = this_host.new_endpoint(v[1]) {
                                        if (*DTNCORE.lock()).get_endpoint(&eid).is_none() {
                                            debug!(
                                                "Attempted to subscribe to unknown endpoint: {}",
                                                eid
                                            );

                                            ws_reply_text!(socket, "404 unknown endpoint");
                                        } else {
                                            if let Some(ep) =
                                                (*DTNCORE.lock()).get_endpoint_mut(&eid)
                                            {
                                                ep.set_delivery_addr(self.tx.clone());
                                            }
                                            debug!("Subscribed to endpoint: {}", eid);
                                            if self.endpoints.is_none() {
                                                self.endpoints = Some(HashSet::new());
                                            }
                                            if let Some(endpoints) = &mut self.endpoints {
                                                endpoints.insert(eid);
                                            }

                                            ws_reply_text!(socket, "200 subscribed");
                                        }
                                    } else {
                                        debug!(
                                            "Invalid endpoint combination: {} and {}",
                                            this_host, v[1]
                                        );

                                        ws_reply_text!(socket, "400 invalid endpoint combination");
                                    }
                                }
                            } else {
                                ws_reply_text!(socket, "400 endpoint is missing");
                            }
                        }
                        _ => {
                            ws_reply_text!(socket, format!("501 unknown command: {:?}", m));
                        }
                    }
                } else {
                }
            }

            Message::Binary(bin) => {
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
                            debug!("sent bundle");

                            ws_reply_text!(
                                socket,
                                format!("200 Sent payload with {} bytes", bin.len())
                            );
                        } else {
                            ws_reply_text!(socket, "400 Invalid binary bundle");
                        }
                    }
                    WsReceiveMode::Data(format) => {
                        let send_data = match format {
                            DataReceiveFormat::CBOR => serde_cbor::from_slice::<WsSendData>(&bin)
                                .map_err(|_| "error parsing cbor"),
                            DataReceiveFormat::JSON => serde_json::from_slice::<WsSendData>(&bin)
                                .map_err(|_| "error parsing json"),
                        };
                        if let Ok(send_req) = send_data {
                            //let src = (*CONFIG.lock()).host_eid.clone();
                            let bcf = if send_req.delivery_notification {
                                BundleControlFlags::BUNDLE_MUST_NOT_FRAGMENTED
                                    | BundleControlFlags::BUNDLE_STATUS_REQUEST_DELIVERY
                            } else {
                                BundleControlFlags::BUNDLE_MUST_NOT_FRAGMENTED
                            };
                            let dst = EndpointID::try_from(send_req.dst.clone());
                            let src = EndpointID::try_from(send_req.src.clone());
                            if dst.is_err() || src.is_err() {
                                warn!(
                                    "Received data with invalid src ({}) or destination ({})",
                                    send_req.src, send_req.dst
                                );
                                return Ok(());
                            }
                            let src2 = src.unwrap();
                            let pblock = bp7::primary::PrimaryBlockBuilder::default()
                                .bundle_control_flags(bcf.bits())
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
                                    bp7::canonical::new_payload_block(
                                        BlockControlFlags::empty(),
                                        send_req.data.to_owned(),
                                    ),
                                    bp7::canonical::new_hop_count_block(
                                        2,
                                        BlockControlFlags::empty(),
                                        32,
                                    ),
                                ])
                                .build()
                                .unwrap();
                            bndl.set_crc(bp7::crc::CRC_NO);

                            debug!(
                                "Sending bundle {} from data frame to {} from WS",
                                bndl.id(),
                                bndl.primary.destination
                            );
                            //let mut rt = tokio::runtime::Runtime::new().unwrap();
                            //rt.block_on(async { crate::core::processing::send_bundle(bndl).await });
                            let rt = tokio::runtime::Handle::current();
                            rt.spawn(
                                async move { crate::core::processing::send_bundle(bndl).await },
                            );
                            debug!("sent bundle");
                            //crate::core::processing::send_through_task(bndl);
                            ws_reply_text!(
                                socket,
                                format!("200 Sent payload with {} bytes", b_len)
                            );
                        } else {
                            ws_reply_text!(socket, "400 Unexpected binary");
                        }
                    }
                }
            }

            Message::Ping(msg) => {
                self.hb = Instant::now();
                let ping = Message::Ping(msg);
                if let Err(err) = socket.send(ping).await {
                    bail!("err sending pong: {}", err);
                }
            }

            Message::Pong(_) => {
                self.hb = Instant::now();
            }

            Message::Close(_) => {
                bail!("received close message");
            }
        }

        Ok(())
    }
    pub async fn fetch_new_bundles(&mut self, socket: mpsc::Sender<Message>) {
        debug!("delivering bundles for endpoint(s)");
        let mut senders = Vec::new();
        if let Some(endpoints) = self.endpoints.clone() {
            for eid in endpoints {
                if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&eid) {
                    if let Some(mut bundle) = aa.pop() {
                        let recv_data = match self.mode {
                            WsReceiveMode::Bundle => bundle.to_cbor(),
                            WsReceiveMode::Data(format) => {
                                if bundle.payload().is_none() {
                                    // No payload -> nothing to deliver to client
                                    // In bundle mode delivery happens because custom canoncial bocks could be present
                                    continue;
                                }
                                let recv = WsRecvData {
                                    bid: bundle.id(),
                                    src: bundle.primary.source.to_string(),
                                    dst: bundle.primary.destination.to_string(),
                                    data: bundle.payload().unwrap().to_vec(),
                                };
                                match format {
                                    DataReceiveFormat::CBOR => serde_cbor::to_vec(&recv)
                                        .expect("Fatal error encoding WsRecvData"),
                                    DataReceiveFormat::JSON => serde_json::to_vec(&recv)
                                        .expect("Fatal error encoding WsRecvData"),
                                }
                            }
                        };
                        let job = socket.send(Message::Binary(recv_data)); //.await;
                        senders.push(job)
                        //ctx.binary(recv_data);
                    }
                }
            }
        }
        for job in senders {
            let _res = job.await;
        }
    }
}

/// Application Agent sends this messages to session
#[derive(Debug)]
pub struct BundleDelivery(pub Bundle);
