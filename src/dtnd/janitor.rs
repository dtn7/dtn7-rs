use log::debug;

fn janitor() {
    debug!("running janitor");
    //DTNCORE.lock().unwrap().process();
    // TODO: reimpl janitor with new processing
    debug!("cleaning up peers");
    crate::core::process_peers();

    debug!("reprocessing bundles");
    crate::core::process_bundles();
}

pub fn spawn_janitor() {
    crate::dtnd::cron::spawn_timer((*crate::CONFIG.lock()).janitor_interval, janitor);
}
