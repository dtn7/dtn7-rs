use super::{janitor, rest, service_discovery};
use crate::core::core::DtnCore;
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
    if let DtnCmdResult::DtnCore(tx3, mut core) = rx2.recv().unwrap() {
        f(&mut core);
        tx3.send(core).expect("IPC Error");
    }
}
fn spawn_test_sender(tx: Sender<DtnCmd>) {
    let (tx2, rx2) = mpsc::channel();
    tx.send(DtnCmd::DtnCore(tx2)).expect("IPC Error");

    if let DtnCmdResult::DtnCore(tx3, mut core) = rx2.recv().unwrap() {
        dbg!(core.process());
        tx3.send(core).expect("IPC Error");
    }

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

fn start_convergencylayers(core: &DtnCore, tx: Sender<DtnCmd>) {
    info!("Starting convergency layers");
    for cl in &core.cl_list {
        info!("Setup {}", cl);
        cl.setup(core, tx.clone());
    }
}

pub fn start_dtnd(core: DtnCore) {
    // Blocks the thread until the future runs to completion (which will never happen).
    //tokio::run(future.map_err(|err| panic!("{:?}", err)));

    tokio::run(lazy(move || {
        let (tx, rx) = mpsc::channel();

        start_convergencylayers(&core, tx.clone());

        /*let tx2 = tx.clone();
        tokio::spawn(lazy(move || {
            spawn_test_sender(tx2);
            Ok(())
        }));*/
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
