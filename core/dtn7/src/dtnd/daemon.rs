use std::convert::TryFrom;

use super::{httpd, janitor};
use crate::cla::ecla::processing::start_ecla;
use crate::cla::ConvergenceLayerAgent;
use crate::core::application_agent::SimpleApplicationAgent;
use crate::dtnconfig::DtnConfig;
use crate::ipnd::neighbour_discovery;
use crate::{cla_add, peers_add};
use crate::{CLAS, CONFIG, DTNCORE, STORE};
use bp7::EndpointID;
use log::{error, info};

/*
use crate::core::core::DtnCore;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub enum DtnCmd {
    DtnCore(Sender<DtnCmdResult>),
}

#[derive(Debug)]
pub enum DtnCmdResult {
    DtnCore(Sender<DtnCore>, DtnCore),
}

pub fn access_core<F>(tx: Sender<DtnCmd>, mut f: F)
where
    F: FnMut(&mut DtnCore),
{
    let (tx2, rx2) = mpsc::channel();
    tx.send(DtnCmd::DtnCore(tx2)).unwrap();
    let DtnCmdResult::DtnCore(tx3, mut core) = rx2.recv().expect("Couldn't access dtn core!");
    f(&mut core);
    tx3.send(core).expect("IPC Error");
}

fn spawn_core_daemon(rx: Receiver<DtnCmd>) {
    for received in rx {
        //println!("Got: {:?}", received);
        match received {
            DtnCmd::DtnCore(tx) => {
                let (tx2, rx2) = mpsc::channel();
                tx.send(DtnCmdResult::DtnCore(tx2, core))
                    .expect("IPC Error");
                core = rx2.recv().unwrap();
            }
        };
    }
}*/

// this function is only called once during startup
// therefore it should be safe to hold the lock
#[allow(clippy::await_holding_lock)]
async fn start_convergencylayers() {
    info!("Starting convergency layers");

    for cl in &mut (*CLAS.lock()) {
        info!("Setup {}", cl);
        cl.setup().await;
    }
}

pub async fn start_dtnd(cfg: DtnConfig) -> anyhow::Result<()> {
    {
        (*CONFIG.lock()).set(cfg);
    }
    info!("Local Node ID: {}", (*CONFIG.lock()).host_eid);

    info!("Work Dir: {:?}", (*CONFIG.lock()).workdir);

    let db = (*CONFIG.lock()).db.clone();
    info!("DB Backend: {}", db);

    (*STORE.lock()) = crate::core::store::new(&db);

    info!(
        "Announcement Interval: {}",
        humantime::format_duration((*CONFIG.lock()).announcement_interval)
    );

    info!(
        "Janitor Interval: {}",
        humantime::format_duration((*CONFIG.lock()).janitor_interval)
    );

    info!(
        "Peer Timeout: {}",
        humantime::format_duration((*CONFIG.lock()).peer_timeout)
    );

    info!("Web Port: {}", (*CONFIG.lock()).webport);
    info!("IPv4: {}", (*CONFIG.lock()).v4);
    info!("IPv6: {}", (*CONFIG.lock()).v6);

    info!(
        "Generate Status Reports: {}",
        (*CONFIG.lock()).generate_status_reports
    );

    let routing = (*CONFIG.lock()).routing.clone();
    (*DTNCORE.lock()).routing_agent = crate::routing::new(&routing);

    info!("RoutingAgent: {}", routing);

    let routing_options = (*CONFIG.lock()).routing_settings.clone();
    info!("RoutingOptions: {:?}", routing_options);

    let clas = (*CONFIG.lock()).clas.clone();
    for (cla, local_settings) in &clas {
        info!("Adding CLA: {:?}", cla);
        cla_add(crate::cla::new(cla, Some(local_settings)));
    }

    for s in &(*CONFIG.lock()).statics {
        info!(
            "Adding static peer: {}://{}/{}",
            s.cla_list[0].0,
            s.addr,
            s.eid.node().unwrap()
        );
        peers_add(s.clone());
    }

    let local_host_id = (*CONFIG.lock()).host_eid.clone();
    (*DTNCORE.lock())
        .register_application_agent(SimpleApplicationAgent::with(local_host_id.clone()).into());
    for e in &(*CONFIG.lock()).endpoints {
        let eid = if let Ok(eid) = EndpointID::try_from(e.clone()) {
            // TODO: add check if non-local ID that service name is non-singleton ('~') for naming scheme dtn
            eid
        } else {
            local_host_id
                .new_endpoint(e)
                .expect("Error constructing new endpoint")
        };

        (*DTNCORE.lock()).register_application_agent(SimpleApplicationAgent::with(eid).into());
    }
    start_convergencylayers().await;
    if CONFIG.lock().janitor_interval.as_micros() != 0 {
        janitor::spawn_janitor();
    }

    let dn = CONFIG.lock().disable_neighbour_discovery;
    let interval = CONFIG.lock().announcement_interval.as_micros();
    if !dn && interval != 0 {
        if let Err(errmsg) = neighbour_discovery::spawn_neighbour_discovery().await {
            error!("Error spawning service discovery: {:?}", errmsg);
        }
    }

    if (*CONFIG.lock()).ecla_enable {
        let ecla_port = (*CONFIG.lock()).ecla_tcp_port;
        start_ecla(ecla_port).await;
    }

    httpd::spawn_httpd().await?;
    Ok(())
}
