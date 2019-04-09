use super::daemon::*;
use crate::dtnconfig;
use crate::core::core::DtnCore;
use log::{debug, error, info, trace, warn};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use tokio::prelude::*;
use tokio::timer::Interval;

fn janitor(core: &mut DtnCore) {
    debug!("running janitor");
    core.process();
}

pub fn spawn_janitor(tx: Sender<DtnCmd>) {
    let tx = std::sync::Mutex::new(tx.clone());
    let task = Interval::new(Instant::now(), Duration::from_millis(dtnconfig::CONFIG.lock().unwrap().janitor_interval))
        .for_each(move |_instant| {
            access_core(tx.lock().unwrap().clone(), |c| {
                janitor(c);
            });
            Ok(())
        })
        .map_err(|e| panic!("interval errored; err={:?}", e));
    tokio::spawn(task);
}
