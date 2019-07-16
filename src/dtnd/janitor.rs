use crate::DTNCORE;
use log::{debug};
use std::time::{Duration, Instant};
use tokio::prelude::*;
use tokio::timer::Interval;

fn janitor() {
    debug!("running janitor");
    //DTNCORE.lock().unwrap().process();
    // TODO: reimpl janitor with new processing
}

pub fn spawn_janitor() {
    let task = Interval::new(
        Instant::now(),
        Duration::from_millis(crate::CONFIG.lock().unwrap().janitor_interval),
    )
    .for_each(move |_instant| {
        janitor();

        Ok(())
    })
    .map_err(|e| panic!("interval errored; err={:?}", e));
    tokio::spawn(task);
}
