use crate::cla;
use crate::core::bundlepack::*;
use crate::core::*;
use crate::peer_find_by_remote;
use crate::peers_cla_for_node;
use crate::routing::RoutingNotifcation;
use crate::routing_notify;
use crate::store_push_bundle;
use crate::store_remove;
use crate::CONFIG;
use crate::DTNCORE;
use crate::{is_local_node_id, STATS};

use bp7::administrative_record::*;
use bp7::bundle::*;
use bp7::flags::*;
use bp7::CanonicalData;
use bp7::BUNDLE_AGE_BLOCK;

use anyhow::{bail, Result};
use log::{debug, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tokio::sync::mpsc::channel;

// transmit an outbound bundle.
pub async fn send_bundle(bndl: Bundle) {
    tokio::spawn(async move {
        if let Err(err) = store_push_bundle(&bndl) {
            warn!("Transmission failed: {}", err);
            return;
        }
        if let Err(err) = transmit(bndl.into()).await {
            warn!("Transmission failed: {}", err);
        }
    });
}

pub fn send_through_task(bndl: Bundle) {
    let mut stask = crate::SENDERTASK.lock();
    if stask.is_none() {
        let (tx, rx) = channel(50);
        tokio::spawn(sender_task(rx));
        *stask = Some(tx);
    }
    let tx = stask.as_ref().unwrap().clone();
    //let mut rt = tokio::runtime::Runtime::new().unwrap();
    let rt = tokio::runtime::Handle::current();
    rt.spawn(async move { tx.send(bndl).await });
}

pub async fn send_through_task_async(bndl: Bundle) {
    let mut stask = crate::SENDERTASK.lock();
    if stask.is_none() {
        let (tx, rx) = channel(50);
        tokio::spawn(sender_task(rx));
        *stask = Some(tx);
    }
    let tx = stask.as_ref().unwrap().clone();

    if let Err(err) = tx.send(bndl).await {
        warn!("Transmission failed: {}", err);
    }
}
pub async fn sender_task(mut rx: tokio::sync::mpsc::Receiver<Bundle>) {
    while let Some(bndl) = rx.recv().await {
        debug!("sending bundle through task channel");
        send_bundle(bndl).await;
    }
}

// starts the transmission of an outbounding bundle pack. Therefore
// the source's endpoint ID must be dtn:none or a member of this node.
pub async fn transmit(mut bp: BundlePack) -> Result<()> {
    info!("Transmission of bundle requested: {}", bp.id());

    bp.add_constraint(Constraint::DispatchPending);
    bp.sync()?;

    let src = &bp.source;
    if src != &bp7::EndpointID::none() && (*DTNCORE.lock()).get_endpoint_mut(src).is_none() {
        info!(
            "Bundle's source is neither dtn:none nor an endpoint of this node: {} {}",
            bp.id(),
            src
        );

        delete(bp, NO_INFORMATION).await?;
    } else {
        dispatch(bp).await?;
    }
    Ok(())
}

// handle received/incoming bundles.
pub async fn receive(mut bndl: Bundle) -> Result<()> {
    if store_has_item(&bndl.id()) {
        debug!(
            "Received an already known bundle, skip processing: {}",
            bndl.id()
        );

        // bundleDeletion is _not_ called because this would delete the already
        // stored BundlePack.
        return Ok(());
    } else {
        info!("Received new bundle: {}", bndl.id());
    }

    if let Err(err) = store_push_bundle(&bndl) {
        bail!("error adding received bundle: {} {}", bndl.id(), err);
    }
    let mut bp = BundlePack::from(&bndl);
    bp.add_constraint(Constraint::DispatchPending);
    bp.sync()?;

    if bndl
        .primary
        .bundle_control_flags
        .contains(BundleControlFlags::BUNDLE_STATUS_REQUEST_RECEPTION)
        && !bndl.is_administrative_record()
        && (*CONFIG.lock()).generate_status_reports
    {
        send_status_report(&bp, RECEIVED_BUNDLE, NO_INFORMATION).await;
    }
    let mut remove_idx = Vec::new();
    let mut index = 0;
    for cb in bndl.canonicals.iter() {
        if cb.block_type < 11 {
            // TODO: fix magic number to check for a known block type
            continue;
        }
        warn!(
            "Bundle's canonical block is unknown: {} {}",
            bp.id(),
            cb.block_type
        );
        let flags = cb.block_control_flags.flags();
        if flags.contains(BlockControlFlags::BLOCK_STATUS_REPORT) {
            info!(
                "Bundle's unknown canonical block requested reporting: {} {}",
                bp.id(),
                cb.block_type
            );
            if (*CONFIG.lock()).generate_status_reports {
                send_status_report(&bp, RECEIVED_BUNDLE, BLOCK_UNINTELLIGIBLE).await;
            } else {
                info!("Generation of status reports disabled, ignoring request");
            }
        }
        if flags.contains(BlockControlFlags::BLOCK_DELETE_BUNDLE) {
            info!(
                "Bundle's unknown canonical block requested bundle deletion: {} {}",
                bp.id(),
                cb.block_type
            );
            delete(bp, BLOCK_UNINTELLIGIBLE).await?;
            return Ok(());
        }
        if flags.contains(BlockControlFlags::BLOCK_REMOVE) {
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
        bndl.canonicals.remove(i);
    }
    if let Err(err) = store_push_bundle(&bndl) {
        bail!("error adding received bundle: {} {}", bndl.id(), err);
    }
    if let Err(err) = dispatch(bp).await {
        warn!("Dispatching failed: {}", err);
    }
    Ok(())
}

// handle the dispatching of received bundles.
pub async fn dispatch(bp: BundlePack) -> Result<()> {
    info!("Dispatching bundle: {}", bp.id());

    routing_notify(RoutingNotifcation::IncomingBundle(
        &store_get_bundle(bp.id()).unwrap(),
    ));

    if (*DTNCORE.lock()).is_in_endpoints(&bp.destination)
    // TODO: lookup here AND in local delivery, optmize for just one
    {
        local_delivery(bp.clone()).await?;
    }
    if !is_local_node_id(&bp.destination) {
        forward(bp).await?;
    }
    Ok(())
}

async fn handle_hop_count_block(mut bundle: Bundle) -> Result<Bundle> {
    let bid = bundle.id();
    if let Some(hc) = bundle.extension_block_by_type_mut(bp7::canonical::HOP_COUNT_BLOCK) {
        if hc.hop_count_increase() {
            let (hc_limit, hc_count) = hc
                .hop_count_get()
                .expect("hop count data missing from hop count block");
            debug!(
                "Bundle contains an hop count block: {} {} {}",
                &bid, hc_limit, hc_count
            );
            if hc.hop_count_exceeded() {
                warn!(
                    "Bundle contains an exceeded hop count block: {} {} {}",
                    &bid, hc_limit, hc_count
                );
                delete(bundle.into(), HOP_LIMIT_EXCEEDED).await?;
                bail!("hop count exceeded");
            }
        }
    }
    Ok(bundle)
}
async fn handle_primary_lifetime(bundle: &Bundle) -> Result<()> {
    if bundle.primary.is_lifetime_exceeded() {
        warn!(
            "Bundle's primary block's lifetime is exceeded: {} {:?}",
            bundle.id(),
            bundle.primary
        );
        delete(bundle.into(), LIFETIME_EXPIRED).await?;
        bail!("lifetime exceeded");
    }
    Ok(())
}
/// UpdateBundleAge updates the bundle's Bundle Age block based on its reception
/// timestamp, if such a block exists.
pub fn update_bundle_age(bundle: &mut Bundle) -> Option<u64> {
    let bid = bundle.id();
    if let Some(block) = bundle.extension_block_by_type_mut(BUNDLE_AGE_BLOCK) {
        let mut new_age = 0; // TODO: lost fight with borrowchecker
        let bp = store_get_metadata(&bid)?;

        if let CanonicalData::BundleAge(age) = block.data() {
            let offset = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis() as u64
                - bp.received_time;
            new_age = age + offset;
        }
        if new_age != 0 {
            block.set_data(CanonicalData::BundleAge(new_age));
            return Some(new_age);
        }
    }
    None
}
async fn handle_bundle_age_block(mut bundle: Bundle) -> Result<Bundle> {
    if let Some(age) = update_bundle_age(&mut bundle) {
        if std::time::Duration::from_micros(age) >= bundle.primary.lifetime {
            warn!("Bundle's lifetime has expired: {}", bundle.id());
            delete(bundle.into(), LIFETIME_EXPIRED).await?;
            bail!("age block lifetime exceeded");
        }
    }
    Ok(bundle)
}

async fn handle_previous_node_block(mut bundle: Bundle) -> Result<Bundle> {
    if let Some(pnb) = bundle.extension_block_by_type_mut(bp7::canonical::PREVIOUS_NODE_BLOCK) {
        let prev_eid = &pnb
            .previous_node_get()
            .expect("no previoud node EID found!")
            .clone();
        let local_eid = (*CONFIG.lock()).host_eid.clone();
        pnb.previous_node_update(local_eid.clone());
        debug!(
            "Previous Node Block was updated: {} {} {}",
            bundle.id(),
            prev_eid,
            local_eid
        );
    } else {
        // according to rfc always add a previous node block
        let local_eid = (*CONFIG.lock()).host_eid.clone();
        let pnb = bp7::canonical::new_previous_node_block(0, BlockControlFlags::empty(), local_eid);
        bundle.add_canonical_block(pnb);
    }
    Ok(bundle)
}
// forward a bundle pack's bundle to another node.
pub async fn forward(mut bp: BundlePack) -> Result<()> {
    let bpid = bp.id().to_string();

    info!("Forward request for bundle: {}", bpid);

    bp.add_constraint(Constraint::ForwardPending);
    bp.remove_constraint(Constraint::DispatchPending);
    debug!("updating bundle info in store: {}", bpid);
    bp.sync()?;

    debug!("Handle lifetime");
    let bndl = store_get_bundle(&bpid);
    if bndl.is_none() {
        bail!("bundle not found: {}", bpid);
    }
    let mut bndl = bndl.unwrap();
    handle_primary_lifetime(&bndl).await?;

    let mut delete_afterwards = true;
    let bundle_sent = Arc::new(AtomicBool::new(false));
    let mut nodes: Vec<cla::ClaSender> = Vec::new();

    debug!("Check delivery");
    // direct delivery possible?
    if let Some(direct_node) = peers_cla_for_node(&bp.destination) {
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
        bndl = handle_hop_count_block(bndl).await?;

        debug!("Handle previous node block");
        // Handle previous node block
        bndl = handle_previous_node_block(bndl).await?;
        debug!("Handle bundle age block");
        // Handle bundle age block
        bndl = handle_bundle_age_block(bndl).await?;

        //let wg = WaitGroup::new();
        let mut wg = Vec::new();
        let bundle_data = bndl.to_cbor();
        debug!("nodes: {:?}", nodes);
        for n in nodes {
            //let wg = wg.clone();
            let bd = bundle_data.clone(); // TODO: optimize cloning away, reference should do
            let bpid = bpid.clone();
            //let bp2 = bp.clone();
            let bundle_sent = std::sync::Arc::clone(&bundle_sent);
            let n = n.clone();
            let task_handle = tokio::spawn(async move {
                info!(
                    "Sending bundle to a CLA: {} {} {}",
                    &bpid, n.remote, n.agent
                );
                if n.transfer(&[bd]).await {
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
                    // if (*CONFIG.lock()).generate_service_reports {
                    //    send_status_report(&bp2, FORWARDED_BUNDLE, TRANSMISSION_CANCELED);
                    // }
                }
                //drop(wg);
            });
            wg.push(task_handle);
        }
        use futures::future::join_all;

        join_all(wg).await;
        //wg.wait();

        // Reset hop count block
        /*if let Some(hc) = bndl.extension_block_by_type_mut(bp7::canonical::HOP_COUNT_BLOCK) {
            if let Some((hc_limit, mut hc_count)) = hc.hop_count_get() {
                hc_count -= 1;
                hc.set_data(bp7::canonical::CanonicalData::HopCount(hc_limit, hc_count));
                debug!(
                    "Bundle's hop count block was resetted: {} {} {}",
                    &bpid, hc_limit, hc_count
                );
            }
        }*/
        if bundle_sent.load(Ordering::Relaxed) {
            if bndl
                .primary
                .bundle_control_flags
                .contains(BundleControlFlags::BUNDLE_STATUS_REQUEST_FORWARD)
                && !bndl.is_administrative_record()
                && (*CONFIG.lock()).generate_status_reports
            {
                send_status_report(&bp, FORWARDED_BUNDLE, NO_INFORMATION).await;
            }
            if delete_afterwards {
                store_remove(&bpid);
            } else if bndl.is_administrative_record() {
                // TODO: always inspect all bundles, should be configurable
                is_administrative_record_valid(&bndl);
                contraindicated(bp)?;
            }
        } else {
            info!("Failed to forward bundle to any CLA: {}", bp.id());
            contraindicated(bp)?;
        }
    }
    Ok(())
}

pub async fn local_delivery(mut bp: BundlePack) -> Result<()> {
    info!("Received bundle for local delivery: {}", bp.id());

    let bndl = store_get_bundle(bp.id());
    if bndl.is_none() {
        bail!("bundle not found");
    }
    let bndl = bndl.unwrap();

    if bp.administrative && !is_administrative_record_valid(&bndl) {
        delete(bp, NO_INFORMATION).await?;
        bail!("Empty administrative record");
    }
    bp.add_constraint(Constraint::LocalEndpoint);
    bp.sync()?;
    if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&bp.destination) {
        info!("Delivering {}", bp.id());
        aa.push(&bndl);
        (*STATS.lock()).delivered += 1;
    }
    if is_local_node_id(&bp.destination) {
        if bndl
            .primary
            .bundle_control_flags
            .contains(BundleControlFlags::BUNDLE_STATUS_REQUEST_DELIVERY)
            && !bndl.is_administrative_record()
            && (*CONFIG.lock()).generate_status_reports
        {
            send_status_report(&bp, DELIVERED_BUNDLE, NO_INFORMATION).await;
        }
        // TODO: might not be okay to clear if it was a group message, check in various setups
        bp.clear_constraints();
    } else {
        info!(
            "Add forwarding constraint again as bundle is non-local destination: {}",
            bp.id()
        );
        bp.add_constraint(Constraint::ForwardPending);
    }
    bp.sync()?;
    Ok(())
}
pub fn contraindicated(mut bp: BundlePack) -> Result<()> {
    info!("Bundle marked for contraindication: {}", bp.id());
    bp.add_constraint(Constraint::Contraindicated);
    bp.sync()?;
    Ok(())
}

pub async fn delete(mut bp: BundlePack, reason: StatusReportReason) -> Result<()> {
    let bndl = store_get_bundle(bp.id());
    if bndl.is_none() {
        bail!("bundle not found");
    }
    let bndl = bndl.unwrap();
    if bndl
        .primary
        .bundle_control_flags
        .contains(BundleControlFlags::BUNDLE_STATUS_REQUEST_DELETION)
        && !bndl.is_administrative_record()
        && (*CONFIG.lock()).generate_status_reports
    {
        send_status_report(&bp, DELETED_BUNDLE, reason).await;
    }
    bp.clear_constraints();
    info!("Bundle marked for deletion: {}", bp.id());
    bp.sync()?;
    Ok(())
}

fn is_administrative_record_valid(bundle: &Bundle) -> bool {
    if !bundle.is_administrative_record() {
        warn!(
            "Bundle does not contain an administrative record: {}",
            bundle.id()
        );
        return false;
    }

    let payload = bundle.extension_block_by_type(bp7::PAYLOAD_BLOCK);
    if payload.is_none() {
        warn!(
            "Bundle with an administrative record flag misses payload block: {}",
            bundle.id()
        );
        return false;
    }
    match payload.unwrap().data() {
        bp7::canonical::CanonicalData::Data(data) => {
            match serde_cbor::from_slice::<AdministrativeRecord>(data) {
                Ok(ar) => {
                    info!(
                        "Received bundle contains an administrative record: {} {:?}",
                        bundle.id(),
                        ar
                    );
                    // Currently there are only status reports. This must be changed if more
                    // types of administrative records are introduced.
                    inspect_status_report(&bundle.id(), ar);
                    true
                }
                Err(ar) => {
                    warn!(
                        "Bundle with an administrative record could not be parsed: {} {:?}",
                        bundle.id(),
                        ar
                    );
                    false
                }
            }
        }
        _ => {
            warn!(
                "Bundle with an administrative record could not be parsed: {}",
                bundle.id()
            );
            false
        }
    }
}

fn inspect_status_report(bid: &str, ar: AdministrativeRecord) {
    if let AdministrativeRecord::BundleStatusReport(bsr) = &ar {
        let sips = &bsr.status_information;
        if sips.is_empty() {
            warn!(
                "Administrative record contains no status information: {} {:?}",
                bid, ar
            );
            return;
        }
        if !store_has_item(&bsr.refbundle()) {
            warn!("Status Report's bundle is unknown: {} {:?}", bid, ar);
            return;
        }
        if sips.len() != bp7::administrative_record::MAX_STATUS_INFORMATION_POS as usize {
            warn!(
                "Status Report's number of status information is invalid: {} {:?}",
                bid,
                sips.len()
            );
            return;
        }
        for (i, sip) in sips.iter().enumerate() {
            debug!("Parsing Status Report: {} #{} {:?} {:?}", bid, i, bsr, sip);
            match i as u32 {
                bp7::administrative_record::RECEIVED_BUNDLE => {}
                bp7::administrative_record::FORWARDED_BUNDLE => {}
                bp7::administrative_record::DELETED_BUNDLE => {}
                bp7::administrative_record::DELIVERED_BUNDLE => {
                    info!(
                        "Status Report indicated bundle delivery: {} {}",
                        bid,
                        bsr.refbundle()
                    );
                    store_remove(&bsr.refbundle());
                }
                _ => {
                    warn!(
                        "Status Report has unknown status information code: {} #{}",
                        bid, i,
                    );
                }
            }
        }
    } else {
        warn!("No bundle status information found: {} {:?}", bid, ar);
    }
}

// SendStatusReport creates a new status report in response to the given
// BundlePack and transmits it.
async fn send_status_report(
    bp: &BundlePack,
    status: StatusInformationPos,
    reason: StatusReportReason,
) {
    let bndl = store_get_bundle(bp.id());
    if bndl.is_none() {
        warn!("bundle not found when sending status report: {}", bp.id());
        return;
    }
    let bndl = bndl.unwrap();
    // Don't repond to other administrative records
    if bndl
        .primary
        .bundle_control_flags
        .contains(BundleControlFlags::BUNDLE_ADMINISTRATIVE_RECORD_PAYLOAD)
    {
        return;
    }

    // Don't respond to ourself
    if (*DTNCORE.lock()).is_in_endpoints(&bndl.primary.report_to) {
        return;
    }

    info!(
        "Sending a status report for a bundle: {} {:?} {:?}",
        bp.id(),
        status,
        reason
    );

    let out_bndl = new_status_report_bundle(
        &bndl,
        (*CONFIG.lock()).host_eid.clone(),
        bndl.primary.crc.to_code(),
        status,
        reason,
    );

    if let Err(err) = store_push_bundle(&out_bndl) {
        warn!("Storing new status report failed: {}", err);
        return;
    }
    let mut bp: BundlePack = out_bndl.into();
    bp.add_constraint(Constraint::ForwardPending);
    if let Err(err) = bp.sync() {
        warn!("Sending status report failed: {}", err);
    }
    debug!("Enqueued status report: {}", bp.id());
    // TODO: impl without cycle
    //send_bundle(out_bndl).await;
    //dispatch(out_bndl.into()).await;
    //send_through_task_async(out_bndl).await;
}
