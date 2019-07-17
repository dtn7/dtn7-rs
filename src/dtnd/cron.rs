use std::time::{Duration, Instant};
use tokio::prelude::*;
use tokio::timer::Interval;

pub fn spawn_timer<F>(millis : u64, f : F) 
where F: Fn() + Send + Sync + 'static {
    let task = Interval::new(
        Instant::now(),
        Duration::from_millis(millis),
    )
    .for_each(move |_instant| {
        f();

        Ok(())
    })
    .map_err(|e| panic!("interval errored; err={:?}", e));
    tokio::spawn(task);
}