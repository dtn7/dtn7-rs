use crate::cla;
use crate::core::bundlepack::*;
use crate::CONFIG;
use crate::DTNCORE;
use crate::STATS;
use crate::STORE;

use bp7::administrative_record::*;
use bp7::bundle::BundleValidation;
use bp7::bundle::*;

use core::cmp;
use crossbeam::sync::WaitGroup;
use log::{debug, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

// transmit an outbound bundle.
pub fn send_bundle(bndl: Bundle) {
    transmit(bndl.into());
}

// starts the transmission of an outbounding bundle pack. Therefore
// the source's endpoint ID must be dtn:none or a member of this node.
pub fn transmit(mut bp: BundlePack) {
    info!("Transmission of bundle requested: {}", bp.id());

    // TODO: idKeeper impl
    //c.idKeeper.update(bp.Bundle)

    bp.add_constraint(Constraint::DispatchPending);
    {
        STORE.lock().unwrap().push(&bp);
    }
    let src = &bp.bundle.primary.source;
    if src != &bp7::DTN_NONE && DTNCORE.lock().unwrap().get_endpoint_mut(&src).is_none() {
        info!(
            "Bundle's source is neither dtn:none nor an endpoint of this node: {} {}",
            bp.id(),
            src
        );

        delete(bp, NO_INFORMATION);
    } else {
        dispatch(bp);
    }
}

// handle received/incoming bundles.
pub fn receive(mut bp: BundlePack) {
    info!("Received new bundle: {}", bp.id());

    if STORE.lock().unwrap().has_item(&bp) {
        debug!("Received bundle's ID is already known: {}", bp.id());

        // bundleDeletion is _not_ called because this would delete the already
        // stored BundlePack.
        return;
    }

    info!("Processing new received bundle: {}", bp.id());

    bp.add_constraint(Constraint::DispatchPending);

    {
        STORE.lock().unwrap().push(&bp);
    }

    if bp
        .bundle
        .primary
        .bundle_control_flags
        .has(BUNDLE_STATUS_REQUEST_RECEPTION)
    {
        //c.SendStatusReport(bp, ReceivedBundle, NoInformation)
        // TODO: send status report
        unimplemented!();
    }
    let mut remove_idx = Vec::new();
    let mut index = 0;
    for cb in bp.bundle.canonicals.iter() {
        if cb.block_type < 10 {
            // TODO: fix magic number to check for a known block type
            continue;
        }
        warn!(
            "Bundle's canonical block is unknown: {} {}",
            bp.id(),
            cb.block_type
        );

        if cb.block_control_flags.has(BLOCK_STATUS_REPORT) {
            info!(
                "Bundle's unknown canonical block requested reporting: {} {}",
                bp.id(),
                cb.block_type
            );
            unimplemented!();
            // TODO: handle status request delivery
            // c.SendStatusReport(bp, ReceivedBundle, BlockUnintelligible)
        }
        if cb.block_control_flags.has(BLOCK_DELETE_BUNDLE) {
            info!(
                "Bundle's unknown canonical block requested bundle deletion: {} {}",
                bp.id(),
                cb.block_type
            );
            delete(bp, BLOCK_UNINTELLIGIBLE);
            return;
        }
        if cb.block_control_flags.has(BLOCK_REMOVE) {
            info!(
                "Bundle's unknown canonical block requested to be removed: {} {} {}",
                bp.id(),
                cb.block_number,
                cb.block_type
            );
            remove_idx.push(index);
        }
        index += 1;
    }
    for i in remove_idx {
        // Remove canoncial blocks marked for deletion
        bp.bundle.canonicals.remove(i);
    }

    dispatch(bp);
}

// handle the dispatching of received bundles.
pub fn dispatch(bp: BundlePack) {
    info!("Dispatching bundle: {}", bp.id());

    // TODO: impl routing
    //c.routing.NotifyIncoming(bp)

    if DTNCORE
        .lock()
        .unwrap()
        .get_endpoint_mut(&bp.bundle.primary.destination)
        .is_some()
    // TODO: lookup here AND in local delivery, optmize for just one
    {
        local_delivery(bp);
    } else {
        forward(bp);
    }
}

// forward a bundle pack's bundle to another node.
pub fn forward(mut bp: BundlePack) {
    let bpid = bp.id().clone();

    info!("Bundle will be forwarded: {}", bpid);

    bp.add_constraint(Constraint::ForwardPending);
    bp.remove_constraint(Constraint::DispatchPending);
    {
        STORE.lock().unwrap().push(&bp);
    }
    // Handle hop count block
    if let Some(hc) = bp.bundle.extension_block(bp7::canonical::HOP_COUNT_BLOCK) {
        if hc.hop_count_increase() {
            let (hc_limit, hc_count) = hc
                .hop_count_get()
                .expect("hop count data missing from hop count block");
            debug!(
                "Bundle contains an hop count block: {} {} {}",
                &bpid, hc_limit, hc_count
            );
            if hc.hop_count_exceeded() {
                info!(
                    "Bundle contains an exceeded hop count block: {} {} {}",
                    &bpid, hc_limit, hc_count
                );
                delete(bp, HOP_LIMIT_EXCEEDED);
                return;
            }
        }
    }
    // Handle primary block lifetime
    if bp.bundle.primary.is_lifetime_exceeded() {
        warn!(
            "Bundle's primary block's lifetime is exceeded: {} {:?}",
            bp.id(),
            bp.bundle.primary
        );
        delete(bp, LIFETIME_EXPIRED);
        return;
    }

    // Handle bundle age block
    if let Some(age) = bp.update_bundle_age() {
        if age >= bp.bundle.primary.lifetime {
            warn!("Bundle's lifetime has expired: {}", bp.id());
            delete(bp, LIFETIME_EXPIRED);
            return;
        }
    }
    // Handle previous node block
    if let Some(pnb) = bp
        .bundle
        .extension_block(bp7::canonical::PREVIOUS_NODE_BLOCK)
    {
        let prev_eid = &pnb
            .previous_node_get()
            .expect("no previoud node EID found!")
            .clone();
        let local_eid: &str = &CONFIG.lock().unwrap().nodeid;
        pnb.previous_node_update(format!("dtn://{}", local_eid).into());

        debug!(
            "Previous Node Block was updated: {} {} {}",
            bp.id(),
            prev_eid,
            local_eid
        );
    } else {
        // according to rfc always add a previous node block
        let mut highest_block_number = 0;
        for c in bp.bundle.canonicals.iter() {
            highest_block_number = cmp::max(highest_block_number, c.block_number);
        }
        let local_eid: &str = &CONFIG.lock().unwrap().nodeid;
        let pnb = bp7::canonical::new_previous_node_block(
            highest_block_number + 1,
            0,
            format!("dtn://{}", local_eid).into(),
        );
        bp.bundle.canonicals.push(pnb);
    }
    let mut delete_afterwards = true;
    let bundle_sent = Arc::new(AtomicBool::new(false));
    let mut nodes: Vec<cla::CLA_sender> = Vec::new();

    // direct delivery possible?
    if let Some(direct_node) = crate::core::peers_cla_for_node(&bp.bundle.primary.destination) {
        nodes.push(direct_node);
    } else {
        let (cla_nodes, del) = DTNCORE.lock().unwrap().routing_agent.sender_for_bundle(&bp);
        nodes = cla_nodes;
        delete_afterwards = del;
    }
    let wg = WaitGroup::new();
    let bundle_data = bp.bundle.to_cbor();
    for n in nodes {
        let wg = wg.clone();
        let bd = bundle_data.clone(); // TODO: optimize cloning away, reference should do
        let bpid = bpid.clone();
        let bundle_sent = std::sync::Arc::clone(&bundle_sent);
        thread::spawn(move || {
            info!(
                "Sending bundle to a CLA: {} {} {}",
                &bpid, n.remote, n.agent
            );
            if n.transfer(&vec![bd]) {
                info!(
                    "Sending bundle succeeded: {} {} {}",
                    &bpid, n.remote, n.agent
                );
                bundle_sent.store(true, Ordering::Relaxed);
            } else {
                info!("Sending bundle failed: {} {} {}", &bpid, n.remote, n.agent);
                // TODO: report failure to routing agent
                unimplemented!("report failure to routing agent not implemented!");
            }
            drop(wg);
        });
    }
    wg.wait();

    // Reset hop count block
    if let Some(hc) = bp.bundle.extension_block(bp7::canonical::HOP_COUNT_BLOCK) {
        if let Some((hc_limit, mut hc_count)) = hc.hop_count_get() {
            hc_count -= 1;
            hc.set_data(bp7::canonical::CanonicalData::HopCount(hc_limit, hc_count));
            debug!(
                "Bundle's hop count block was resetted: {} {} {}",
                &bpid, hc_limit, hc_count
            );
        }
    }
    if bundle_sent.load(Ordering::Relaxed) {
        if bp
            .bundle
            .primary
            .bundle_control_flags
            .has(bp7::bundle::BUNDLE_STATUS_REQUEST_FORWARD)
        {
            //c.SendStatusReport(bp, ForwardedBundle, NoInformation)
            // TODO: send status report
            unimplemented!("SendStatusReport(bp, ForwardedBundle, NoInformation) not implemented!");
        }
        if delete_afterwards {
            bp.clear_constraints();
            STORE.lock().unwrap().push(&bp);
        } else if bp.bundle.is_administrative_record() {
            // TODO: always inspect all bundles, should be configurable
            is_administrative_record_valid(&mut bp);
            contraindicated(bp);
        }
    } else {
        info!("Failed to forward bundle to any CLA: {}", bp.id());
        contraindicated(bp);
    }
    dbg!(STORE.lock().unwrap().bundles_status());
}

pub fn local_delivery(mut bp: BundlePack) {
    info!("Received bundle for local delivery: {}", bp.id());

    if bp.bundle.is_administrative_record() {
        unimplemented!("Handling of administrative records in local delivery not implemented!");
    }
    bp.add_constraint(Constraint::LocalEndpoint);
    {
        STORE.lock().unwrap().push(&bp);
    }
    if let Some(aa) = DTNCORE
        .lock()
        .unwrap()
        .get_endpoint_mut(&bp.bundle.primary.destination)
    {
        info!("Delivering {}", bp.id());
        aa.push(&bp.bundle);
        STATS.lock().unwrap().delivered += 1;
    }
    if bp
        .bundle
        .primary
        .bundle_control_flags
        .has(bp7::bundle::BUNDLE_STATUS_REQUEST_DELIVERY)
    {
        unimplemented!();
        // TODO: handle status request delivery
    }
    bp.clear_constraints();
    {
        STORE.lock().unwrap().push(&bp);
    }
}
pub fn contraindicated(mut bp: BundlePack) {
    info!("Bundle marked for contraindication: {}", bp.id());
    bp.add_constraint(Constraint::Contraindicated);
    STORE.lock().unwrap().push(&bp);
}

pub fn delete(mut bp: BundlePack, reason: StatusReportReason) {
    if bp
        .bundle
        .primary
        .bundle_control_flags
        .has(bp7::bundle::BUNDLE_STATUS_REQUEST_DELETION)
    {
        unimplemented!();
        //new_status_report_bundle(bp.bundle);
    }
    bp.clear_constraints();
    info!("Bundle marked for deletion: {}", bp.id());
    {
        STORE.lock().unwrap().push(&bp);
    }
}

fn is_administrative_record_valid(bp: &mut BundlePack) -> bool {
    if !bp.bundle.is_administrative_record() {
        warn!(
            "Bundle does not contain an administrative record: {}",
            bp.id()
        );
        return false;
    }

    let payload = bp.bundle.extension_block(bp7::PAYLOAD_BLOCK);
    if payload.is_none() {
        warn!(
            "Bundle with an administrative record flag misses payload block: {}",
            bp.id()
        );
        return false;
    }
    match payload.unwrap().get_data() {
        bp7::canonical::CanonicalData::Data(data) => {
            let ar = serde_cbor::from_slice::<AdministrativeRecord>(data);
            if ar.is_err() {
                warn!(
                    "Bundle with an administrative record could not be parsed: {} {:?}",
                    bp.id(),
                    ar
                );
                false
            } else {
                info!(
                    "Received bundle contains an administrative record: {} {:?}",
                    bp.id(),
                    ar
                );
                // Currently there are only status reports. This must be changed if more
                // types of administrative records are introduced.
                inspect_status_report(bp, ar.unwrap());

                true
            }
        }
        _ => {
            warn!(
                "Bundle with an administrative record could not be parsed: {}",
                bp.id()
            );
            false
        }
    }
}

fn inspect_status_report(bp: &BundlePack, ar: AdministrativeRecord) {
    if let AdministrativeRecord::BundleStatusReport(bsr) = &ar {
        let sips = &bsr.status_information;
        if sips.is_empty() {
            warn!(
                "Administrative record contains no status information: {} {:?}",
                bp.id(),
                ar
            );
            return;
        }
    } else {
        warn!("No bundle status information found: {} {:?}", bp.id(), ar);
    }

    /*
    var bpStores = QueryFromStatusReport(c.store, status)
    if len(bpStores) != 1 {
        log.WithFields(log.Fields{
            "bundle":     bp.ID(),
            "status_rep": status,
            "store_numb": len(bpStores),
        }).Warn("Status Report's bundle is unknown")
        return
    }

    var bpStore = bpStores[0]
    log.WithFields(log.Fields{
        "bundle":        bp.ID(),
        "status_rep":    status,
        "status_bundle": bpStore.ID(),
    }).Debug("Status Report's referenced bundle was loaded")

    for _, sip := range sips {
        log.WithFields(log.Fields{
            "bundle":        bp.ID(),
            "status_rep":    status,
            "status_bundle": bpStore.ID(),
            "information":   sip,
        }).Info("Parsing status report")

        switch sip {
        case ReceivedBundle, ForwardedBundle, DeletedBundle:
            // Nothing to do

        case DeliveredBundle:
            log.WithFields(log.Fields{
                "bundle":        bp.ID(),
                "status_rep":    status,
                "status_bundle": bpStore.ID(),
            }).Info("Status report indicates delivered bundle, deleting bundle")

            bpStore.PurgeConstraints()
            c.store.Push(bpStore)

        default:
            log.WithFields(log.Fields{
                "bundle":        bp.ID(),
                "status_rep":    status,
                "status_bundle": bpStore.ID(),
                "information":   int(sip),
            }).Warn("Status report has an unknown status information code")
        }
    }*/
}
