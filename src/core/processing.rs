use crate::cla;
use crate::core::bundlepack::*;
use crate::core::*;
use crate::peer_find_by_remote;
use crate::peers_cla_for_node;
use crate::routing::RoutingNotifcation;
use crate::routing_notify;
use crate::store_remove;
use crate::CONFIG;
use crate::DTNCORE;
use crate::STATS;

use bp7::administrative_record::*;
use bp7::bundle::BundleValidation;
use bp7::bundle::*;

use anyhow::{bail, Result};
use crossbeam::sync::WaitGroup;
use log::{debug, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

// transmit an outbound bundle.
pub fn send_bundle(bndl: Bundle) {
    thread::spawn(move || {
        if let Err(err) = transmit(bndl.into()) {
            warn!("Transmission failed: {}", err);
        }
    });
}

// starts the transmission of an outbounding bundle pack. Therefore
// the source's endpoint ID must be dtn:none or a member of this node.
pub fn transmit(mut bp: BundlePack) -> Result<()> {
    info!("Transmission of bundle requested: {}", bp.id());

    bp.add_constraint(Constraint::DispatchPending);
    bp.sync()?;
    let src = &bp.bundle.primary.source;
    if src != &bp7::DTN_NONE && (*DTNCORE.lock()).get_endpoint_mut(&src).is_none() {
        info!(
            "Bundle's source is neither dtn:none nor an endpoint of this node: {} {}",
            bp.id(),
            src
        );

        delete(bp, NO_INFORMATION)?;
        Ok(())
    } else {
        dispatch(bp)
    }
}

// handle received/incoming bundles.
pub fn receive(mut bp: BundlePack) -> Result<()> {
    info!("Received new bundle: {}", bp.id());

    if store_has_item(bp.id()) {
        debug!("Received bundle's ID is already known: {}", bp.id());

        // bundleDeletion is _not_ called because this would delete the already
        // stored BundlePack.
        return Ok(());
    }

    info!("Processing new received bundle: {}", bp.id());

    bp.add_constraint(Constraint::DispatchPending);
    bp.sync()?;

    if bp
        .bundle
        .primary
        .bundle_control_flags
        .has(BUNDLE_STATUS_REQUEST_RECEPTION)
    {
        send_status_report(&bp, RECEIVED_BUNDLE, NO_INFORMATION);
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
            send_status_report(&bp, RECEIVED_BUNDLE, BLOCK_UNINTELLIGIBLE);
        }
        if cb.block_control_flags.has(BLOCK_DELETE_BUNDLE) {
            info!(
                "Bundle's unknown canonical block requested bundle deletion: {} {}",
                bp.id(),
                cb.block_type
            );
            delete(bp, BLOCK_UNINTELLIGIBLE)?;
            return Ok(());
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

    if let Err(err) = dispatch(bp) {
        warn!("Dispatching failed: {}", err);
    }
    Ok(())
}

// handle the dispatching of received bundles.
pub fn dispatch(bp: BundlePack) -> Result<()> {
    info!("Dispatching bundle: {}", bp.id());

    // TODO: impl routing
    //c.routing.NotifyIncoming(bp)
    routing_notify(RoutingNotifcation::IncomingBundle(&bp.bundle));

    if (*DTNCORE.lock())
        .get_endpoint_mut(&bp.bundle.primary.destination)
        .is_some()
    // TODO: lookup here AND in local delivery, optmize for just one
    {
        local_delivery(bp)
    } else {
        forward(bp)
    }
}

fn handle_hop_count_block(mut bp: BundlePack) -> Result<BundlePack> {
    let bpid = bp.id().to_string();
    if let Some(hc) = bp
        .bundle
        .extension_block_mut(bp7::canonical::HOP_COUNT_BLOCK)
    {
        if hc.hop_count_increase() {
            let (hc_limit, hc_count) = hc
                .hop_count_get()
                .expect("hop count data missing from hop count block");
            debug!(
                "Bundle contains an hop count block: {} {} {}",
                &bpid, hc_limit, hc_count
            );
            if hc.hop_count_exceeded() {
                warn!(
                    "Bundle contains an exceeded hop count block: {} {} {}",
                    &bpid, hc_limit, hc_count
                );
                delete(bp, HOP_LIMIT_EXCEEDED)?;
                bail!("hop count exceeded");
            }
        }
    }
    Ok(bp)
}
fn handle_primary_lifetime(bp: BundlePack) -> Result<BundlePack> {
    if bp.bundle.primary.is_lifetime_exceeded() {
        warn!(
            "Bundle's primary block's lifetime is exceeded: {} {:?}",
            bp.id(),
            bp.bundle.primary
        );
        delete(bp, LIFETIME_EXPIRED)?;
        bail!("lifetime exceeded");
    }
    Ok(bp)
}

fn handle_bundle_age_block(mut bp: BundlePack) -> Result<BundlePack> {
    if let Some(age) = bp.update_bundle_age() {
        if age >= bp.bundle.primary.lifetime {
            warn!("Bundle's lifetime has expired: {}", bp.id());
            delete(bp, LIFETIME_EXPIRED)?;
            bail!("age block lifetime exceeded");
        }
    }
    Ok(bp)
}

fn handle_previous_node_block(mut bp: BundlePack) -> Result<BundlePack> {
    if let Some(pnb) = bp
        .bundle
        .extension_block_mut(bp7::canonical::PREVIOUS_NODE_BLOCK)
    {
        let prev_eid = &pnb
            .previous_node_get()
            .expect("no previoud node EID found!")
            .clone();
        let local_eid: &str = &(*CONFIG.lock()).nodeid;
        pnb.previous_node_update(format!("dtn://{}", local_eid).into());

        debug!(
            "Previous Node Block was updated: {} {} {}",
            bp.id(),
            prev_eid,
            local_eid
        );
    } else {
        // according to rfc always add a previous node block
        let local_eid: &str = &(*CONFIG.lock()).nodeid;
        let pnb =
            bp7::canonical::new_previous_node_block(0, 0, format!("dtn://{}", local_eid).into());
        bp.bundle.add_canonical_block(pnb);
    }
    Ok(bp)
}
// forward a bundle pack's bundle to another node.
pub fn forward(mut bp: BundlePack) -> Result<()> {
    let bpid = bp.id().to_string();

    info!("Forward request for bundle: {}", bpid);

    bp.add_constraint(Constraint::ForwardPending);
    bp.remove_constraint(Constraint::DispatchPending);
    debug!("updating bundle info in store");
    bp.sync()?;

    debug!("Handle lifetime");
    bp = handle_primary_lifetime(bp)?;

    let mut delete_afterwards = true;
    let bundle_sent = Arc::new(AtomicBool::new(false));
    let mut nodes: Vec<cla::ClaSender> = Vec::new();

    debug!("Check delivery");
    // direct delivery possible?
    if let Some(direct_node) = peers_cla_for_node(&bp.bundle.primary.destination) {
        debug!("Attempting direct delivery: {:?}", direct_node);
        nodes.push(direct_node);
    } else {
        let (cla_nodes, del) = (*DTNCORE.lock()).routing_agent.sender_for_bundle(&bp);
        nodes = cla_nodes;
        delete_afterwards = del;
        if !nodes.is_empty() {
            debug!("Attempting forwarding to nodes: {:?}", nodes);
        }
    }
    if nodes.is_empty() {
        debug!("No new peers for forwarding of bundle {}", &bp.id());
    } else {
        debug!("Handle hop count block");
        bp = handle_hop_count_block(bp)?;

        debug!("Handle previous node block");
        // Handle previous node block
        bp = handle_previous_node_block(bp)?;
        debug!("Handle bundle age block");
        // Handle bundle age block
        bp = handle_bundle_age_block(bp)?;

        let wg = WaitGroup::new();
        let bundle_data = bp.bundle.to_cbor();
        for n in nodes {
            let wg = wg.clone();
            let bd = bundle_data.clone(); // TODO: optimize cloning away, reference should do
            let bpid = bpid.clone();
            //let bp2 = bp.clone();
            let bundle_sent = std::sync::Arc::clone(&bundle_sent);
            thread::spawn(move || {
                info!(
                    "Sending bundle to a CLA: {} {} {}",
                    &bpid, n.remote, n.agent
                );
                if n.transfer(&[bd]) {
                    info!(
                        "Sending bundle succeeded: {} {} {}",
                        &bpid, n.remote, n.agent
                    );
                    bundle_sent.store(true, Ordering::Relaxed);
                } else if let Some(node_name) = peer_find_by_remote(&n.remote) {
                    (*DTNCORE.lock())
                        .routing_agent
                        .notify(RoutingNotifcation::SendingFailed(&bpid, &node_name));
                    info!("Sending bundle failed: {} {} {}", &bpid, n.remote, n.agent);
                    // TODO: send status report?
                    //send_status_report(&bp2, FORWARDED_BUNDLE, TRANSMISSION_CANCELED);
                }
                drop(wg);
            });
        }
        wg.wait();
        // Reset hop count block
        if let Some(hc) = bp
            .bundle
            .extension_block_mut(bp7::canonical::HOP_COUNT_BLOCK)
        {
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
                send_status_report(&bp, FORWARDED_BUNDLE, NO_INFORMATION);
            }
            if delete_afterwards {
                bp.clear_constraints();
                bp.sync()?;
            } else if bp.bundle.is_administrative_record() {
                // TODO: always inspect all bundles, should be configurable
                is_administrative_record_valid(&bp);
                contraindicated(bp)?;
            }
        } else {
            info!("Failed to forward bundle to any CLA: {}", bp.id());
            contraindicated(bp)?;
        }
    }
    Ok(())
}

pub fn local_delivery(mut bp: BundlePack) -> Result<()> {
    info!("Received bundle for local delivery: {}", bp.id());

    if bp.bundle.is_administrative_record() && !is_administrative_record_valid(&bp) {
        delete(bp, NO_INFORMATION)?;
        bail!("Empty administrative record");
    }
    bp.add_constraint(Constraint::LocalEndpoint);
    bp.sync()?;
    if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&bp.bundle.primary.destination) {
        info!("Delivering {}", bp.id());
        aa.push(&bp.bundle);
        (*STATS.lock()).delivered += 1;
    }
    if bp
        .bundle
        .primary
        .bundle_control_flags
        .has(bp7::bundle::BUNDLE_STATUS_REQUEST_DELIVERY)
    {
        send_status_report(&bp, DELIVERED_BUNDLE, NO_INFORMATION);
    }
    bp.clear_constraints();
    bp.sync()?;
    Ok(())
}
pub fn contraindicated(mut bp: BundlePack) -> Result<()> {
    info!("Bundle marked for contraindication: {}", bp.id());
    bp.add_constraint(Constraint::Contraindicated);
    bp.sync()?;
    Ok(())
}

pub fn delete(mut bp: BundlePack, reason: StatusReportReason) -> Result<()> {
    if bp
        .bundle
        .primary
        .bundle_control_flags
        .has(bp7::bundle::BUNDLE_STATUS_REQUEST_DELETION)
    {
        send_status_report(&bp, DELETED_BUNDLE, reason);
    }
    bp.clear_constraints();
    info!("Bundle marked for deletion: {}", bp.id());
    bp.sync()?;
    Ok(())
}

fn is_administrative_record_valid(bp: &BundlePack) -> bool {
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
    match payload.unwrap().data() {
        bp7::canonical::CanonicalData::Data(data) => {
            match serde_cbor::from_slice::<AdministrativeRecord>(data) {
                Ok(ar) => {
                    info!(
                        "Received bundle contains an administrative record: {} {:?}",
                        bp.id(),
                        ar
                    );
                    // Currently there are only status reports. This must be changed if more
                    // types of administrative records are introduced.
                    inspect_status_report(bp, ar);
                    true
                }
                Err(ar) => {
                    warn!(
                        "Bundle with an administrative record could not be parsed: {} {:?}",
                        bp.id(),
                        ar
                    );
                    false
                }
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
        if !store_has_item(&bsr.refbundle()) {
            warn!("Status Report's bundle is unknown: {} {:?}", bp.id(), ar);
            return;
        }
        if sips.len() != bp7::administrative_record::MAX_STATUS_INFORMATION_POS as usize {
            warn!(
                "Status Report's number of status information is invalid: {} {:?}",
                bp.id(),
                sips.len()
            );
            return;
        }
        for (i, sip) in sips.iter().enumerate() {
            debug!(
                "Parsing Status Report: {} #{} {:?} {:?}",
                bp.id(),
                i,
                bsr,
                sip
            );
            match i as u32 {
                bp7::administrative_record::RECEIVED_BUNDLE => {}
                bp7::administrative_record::FORWARDED_BUNDLE => {}
                bp7::administrative_record::DELETED_BUNDLE => {}
                bp7::administrative_record::DELIVERED_BUNDLE => {
                    info!(
                        "Status Report indicated bundle delivery: {} {}",
                        bp.id(),
                        bsr.refbundle()
                    );
                    store_remove(&bsr.refbundle());
                }
                _ => {
                    warn!(
                        "Status Report has unknown status information code: {} #{}",
                        bp.id(),
                        i,
                    );
                }
            }
        }
    } else {
        warn!("No bundle status information found: {} {:?}", bp.id(), ar);
    }
}

// SendStatusReport creates a new status report in response to the given
// BundlePack and transmits it.
fn send_status_report(bp: &BundlePack, status: StatusInformationPos, reason: StatusReportReason) {
    // Don't repond to other administrative records
    if bp
        .bundle
        .primary
        .bundle_control_flags
        .has(BUNDLE_ADMINISTRATIVE_RECORD_PAYLOAD)
    {
        return;
    }

    // Don't respond to ourself
    if (*DTNCORE.lock()).is_in_endpoints(&bp.bundle.primary.report_to) {
        return;
    }

    info!(
        "Sending a status report for a bundle: {} {:?} {:?}",
        bp.id(),
        status,
        reason
    );

    let out_bndl = new_status_report_bundle(
        &bp.bundle,
        (*CONFIG.lock()).host_eid.clone(),
        bp.bundle.primary.crc.to_code(),
        status,
        reason,
    );

    send_bundle(out_bndl);
}
