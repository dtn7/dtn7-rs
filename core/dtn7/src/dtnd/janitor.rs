use log::{debug, trace};

async fn janitor() {
    debug!("running janitor");

    trace!("cleaning up peers");
    crate::core::process_peers().await;

    // handled in forwarding
    //debug!("cleaning up store");
    //crate::store_delete_expired();

    trace!("reprocessing bundles");
    /*if let Err(err) = crate::core::process_bundles().await {
        error!("Processing bundles failed: {}", err);
    }*/
    crate::core::process_bundles().await;
}

pub fn spawn_janitor() {
    tokio::spawn(crate::dtnd::cron::spawn_timer(
        crate::CONFIG.lock().janitor_interval,
        janitor,
    ));
}
