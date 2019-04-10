use super::{janitor, rest, service_discovery};
use crate::cla::dummy_cl::*;
use crate::cla::stcp::*;
use crate::core::application_agent::ApplicationAgentData;
use crate::dtnconfig::{DtnConfig, CONFIG};
use crate::DTNCORE;
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
    let routing = CONFIG.lock().unwrap().routing.clone();
    DTNCORE.lock().unwrap().routing_agent = crate::routing::new(&routing);

    info!("RoutingAgent: {}", DTNCORE.lock().unwrap().routing_agent);

    let dcl = DummyConversionLayer::new();
    DTNCORE.lock().unwrap().cl_list.push(Box::new(dcl));
    let stcp = StcpConversionLayer::new();
    DTNCORE.lock().unwrap().cl_list.push(Box::new(stcp));

    for e in &CONFIG.lock().unwrap().endpoints {
        let eid = format!("dtn://{}/{}", DTNCORE.lock().unwrap().nodeid, e);
        DTNCORE
            .lock()
            .unwrap()
            .register_application_agent(ApplicationAgentData::new_with(eid.into()));
    }

    tokio::run(lazy(move || {
        //let (tx, rx) = mpsc::channel();

        start_convergencylayers();

        janitor::spawn_janitor();
        service_discovery::spawn_service_discovery();

        rest::spawn_rest();

        //tokio::spawn(lazy(move || {
        //Ok(());
        //}));
        //spawn_core_daemon(rx);
        Ok(())
    }));
}
