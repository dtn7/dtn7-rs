use super::{janitor, rest, service_discovery};
use crate::core::application_agent::SimpleApplicationAgent;
use crate::dtnconfig::DtnConfig;
use crate::PEERS;
use crate::{CONFIG, DTNCORE};
use futures::future::lazy;
use log::{debug, error, info, warn};
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

fn start_convergencylayers() {
    info!("Starting convergency layers");
    for cl in &mut DTNCORE.lock().unwrap().cl_list {
        info!("Setup {}", cl);
        cl.setup();
    }
}

pub fn start_dtnd(cfg: DtnConfig) {
    {
        CONFIG.lock().unwrap().set(cfg);
    }
    info!("Local Node ID: {}", CONFIG.lock().unwrap().nodeid);

    info!("Peer Timeout: {}", CONFIG.lock().unwrap().peer_timeout);

    info!("Web Port: {}", CONFIG.lock().unwrap().webport);

    let routing = CONFIG.lock().unwrap().routing.clone();
    DTNCORE.lock().unwrap().routing_agent = crate::routing::new(&routing);

    info!("RoutingAgent: {}", DTNCORE.lock().unwrap().routing_agent);

    for cla in &CONFIG.lock().unwrap().clas {
        info!("Adding CLA: {}", cla);
        DTNCORE.lock().unwrap().cl_list.push(crate::cla::new(cla)); // TODO: add custom port support
    }

    for s in &CONFIG.lock().unwrap().statics {
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
        PEERS.lock().unwrap().insert(s.addr, s.clone());
    }
    let my_node_id = CONFIG.lock().unwrap().nodeid.clone();

    DTNCORE
        .lock()
        .unwrap()
        .register_application_agent(SimpleApplicationAgent::new_with(
            format!("dtn://{}", my_node_id).into(),
        ));

    for e in &CONFIG.lock().unwrap().endpoints {
        let eid = format!("dtn://{}/{}", my_node_id, e);
        DTNCORE
            .lock()
            .unwrap()
            .register_application_agent(SimpleApplicationAgent::new_with(eid.into()));
    }

    tokio::run(lazy(move || {
        //let (tx, rx) = mpsc::channel();

        start_convergencylayers();

        if CONFIG.lock().unwrap().janitor_interval != 0 {
            janitor::spawn_janitor();
        }
        if CONFIG.lock().unwrap().announcement_interval != 0 {
            service_discovery::spawn_service_discovery();
        }

        rest::spawn_rest();

        //tokio::spawn(lazy(move || {
        //Ok(());
        //}));
        //spawn_core_daemon(rx);
        Ok(())
    }));
}
