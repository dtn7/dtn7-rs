use super::{httpd, janitor, service_discovery};
use crate::cla_add;
use crate::core::application_agent::SimpleApplicationAgent;
use crate::dtnconfig::DtnConfig;
use crate::peers_add;
use crate::{CONFIG, DTNCORE};
use log::{debug, error, info};

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

async fn start_convergencylayers() {
    info!("Starting convergency layers");

    for cl in &mut (*DTNCORE.lock()).cl_list {
        info!("Setup {}", cl);
        cl.setup().await;
    }
}

pub async fn start_dtnd(cfg: DtnConfig) -> std::io::Result<()> {
    {
        (*CONFIG.lock()).set(cfg);
    }
    info!("Local Node ID: {}", (*CONFIG.lock()).nodeid);

    info!("Peer Timeout: {}", (*CONFIG.lock()).peer_timeout);

    info!("Web Port: {}", (*CONFIG.lock()).webport);

    let routing = (*CONFIG.lock()).routing.clone();
    (*DTNCORE.lock()).routing_agent = crate::routing::new(&routing);

    info!("RoutingAgent: {}", (*DTNCORE.lock()).routing_agent);

    for cla in &(*CONFIG.lock()).clas {
        info!("Adding CLA: {}", cla);
        cla_add(crate::cla::new(cla));
    }

    for s in &(*CONFIG.lock()).statics {
        let port_str = if s.cla_list[0].1.is_some() {
            format!(":{}", s.cla_list[0].1.unwrap())
        } else {
            "".into()
        };
        info!(
            "Adding static peer: {}://{}{}/{}",
            s.cla_list[0].0,
            s.addr,
            port_str,
            s.eid.node_part().unwrap()
        );
        peers_add(s.clone());
    }
    let my_node_id = (*CONFIG.lock()).nodeid.clone();

    (*DTNCORE.lock()).register_application_agent(SimpleApplicationAgent::new_with(
        (*CONFIG.lock()).host_eid.clone(),
    ));

    for e in &(*CONFIG.lock()).endpoints {
        let eid = format!("dtn://{}/{}", my_node_id, e);
        (*DTNCORE.lock()).register_application_agent(SimpleApplicationAgent::new_with(eid.into()));
    }

    start_convergencylayers().await;
    if CONFIG.lock().janitor_interval != 0 {
        janitor::spawn_janitor();
    }
    if CONFIG.lock().announcement_interval != 0 {
        if let Err(errmsg) = service_discovery::spawn_service_discovery().await {
            error!("Error spawning service discovery: {:?}", errmsg);
        }
    }
    httpd::spawn_httpd().await
}
