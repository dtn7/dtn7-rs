use super::{janitor, rest, service_discovery};
use crate::cl::dummy_cl::*;
use crate::cl::stcp::*;
use crate::core::application_agent::ApplicationAgentData;
use crate::core::core::DtnCore;
use crate::dtnconfig::{Config, CONFIG};
use futures::future::lazy;
use log::{debug, error, info, trace, warn};
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
fn spawn_test_sender(tx: Sender<DtnCmd>) {
    let (tx2, rx2) = mpsc::channel();
    tx.send(DtnCmd::DtnCore(tx2)).expect("IPC Error");

    let DtnCmdResult::DtnCore(tx3, mut core) = rx2.recv().expect("Couldn't access dtn core!");
    dbg!(core.process());
    tx3.send(core).expect("IPC Error");

    access_core(tx.clone(), |c| {
        dbg!(c.eids());
    });
    access_core(tx, |c| {
        dbg!(c.bundles());
    });
}

fn spawn_core_daemon(rx: Receiver<DtnCmd>, mut core: DtnCore) {
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
}

fn start_convergencylayers(core: &mut DtnCore, tx: Sender<DtnCmd>) {
    info!("Starting convergency layers");
    for cl in &mut core.cl_list {
        info!("Setup {}", cl);
        cl.setup(tx.clone());
    }
}

pub fn start_dtnd(cfg: Config) {
    CONFIG.lock().unwrap().set(cfg);
    let mut core = DtnCore::new();

    let dcl = DummyConversionLayer::new();
    core.cl_list.push(Box::new(dcl));
    let stcp = StcpConversionLayer::new();
    core.cl_list.push(Box::new(stcp));

    for e in &CONFIG.lock().unwrap().endpoints {
        let eid = format!("dtn://{}/{}", core.nodeid, e);
        core.register_application_agent(ApplicationAgentData::new_with(eid.into()));
    }

    tokio::run(lazy(move || {
        let (tx, rx) = mpsc::channel();

        start_convergencylayers(&mut core, tx.clone());

        janitor::spawn_janitor(tx.clone());
        service_discovery::spawn_service_discovery(tx.clone());

        rest::spawn_rest(tx.clone());

        //tokio::spawn(lazy(move || {
        spawn_core_daemon(rx, core);
        //Ok(());
        //}));
        Ok(())
    }));
}
