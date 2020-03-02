use log::{debug, error};

fn janitor() {
    debug!("running janitor");
    //DTNCORE.lock().unwrap().process();
    // TODO: reimpl janitor with new processing
    debug!("cleaning up peers");
    crate::core::process_peers();

    // handled in forwarding
    //debug!("cleaning up store");
    //crate::store_delete_expired();

    debug!("reprocessing bundles");
    if let Err(err) = crate::core::process_bundles() {
        error!("Processing bundles failed: {}", err);
    }
}

pub fn spawn_janitor() {
    tokio::spawn(crate::dtnd::cron::spawn_timer(
        (*crate::CONFIG.lock()).janitor_interval,
        janitor,
    ));
}
