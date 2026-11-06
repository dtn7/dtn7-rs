use crate::core::bpsec::{self, ProcessingStep, Verdict};
use crate::core::bundlepack::*;
use crate::core::*;
use crate::routing::RoutingNotification;
use crate::store_push_bundle;
use crate::store_remove;
use crate::CONFIG;
use crate::DTNCORE;
use crate::{is_local_node_id, STATS};
use crate::{routing_notify, routing_sender_for_bundle, store_add_bundle_if_unknown};

use bp7::administrative_record::*;
use bp7::bundle::*;
use bp7::flags::*;
use bp7::CanonicalData;
use bp7::{
    CanonicalBlockType, BUNDLE_AGE_BLOCK, HOP_COUNT_BLOCK, PAYLOAD_BLOCK, PREVIOUS_NODE_BLOCK,
};

use anyhow::{bail, Result};
use lazy_static::lazy_static;
use log::trace;
use log::{debug, info, warn};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tokio::sync::mpsc::channel;

lazy_static! {
    /// IDs of the bundles currently being forwarded
    static ref FORWARDING: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
}

/// Marks a bundle as being forwarded until it is dropped. This prevents forwarding
/// the same bundle concurrently, e.g., by dispatch() and the janitor.
struct ForwardingGuard(String);

impl ForwardingGuard {
    /// Returns None if the bundle is already being forwarded.
    fn acquire(bid: &str) -> Option<ForwardingGuard> {
        if FORWARDING.lock().insert(bid.to_string()) {
            Some(ForwardingGuard(bid.to_string()))
        } else {
            None
        }
    }
}

impl Drop for ForwardingGuard {
    fn drop(&mut self) {
        FORWARDING.lock().remove(&self.0);
    }
}

// transmit an outbound bundle.
pub async fn send_bundle(mut bndl: Bundle) {
    tokio::spawn(async move {
        if let Err(err) = validate_bundle(&bndl) {
            warn!("Transmission failed: {}", err);
            return;
        }
        if let Verdict::Drop(_) = bpsec::enforce(ProcessingStep::Transmit, &mut bndl) {
            warn!(
                "Transmission failed: bundle {} violates the security policy",
                bndl.id()
            );
            return;
        }
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
    let rt = tokio::runtime::Handle::current();
    let mut stask = crate::SENDERTASK.lock();
    if stask.is_none() {
        let (tx, rx) = channel(50);
        tokio::spawn(sender_task(rx));
        *stask = Some(tx);
    }
    let tx = stask.as_ref().unwrap().clone();
    //let mut rt = tokio::runtime::Runtime::new().unwrap();
    rt.spawn(async move { tx.send(bndl).await });
}

pub async fn send_through_task_async(bndl: Bundle) {
    let tx = {
        let mut stask = crate::SENDERTASK.lock();
        if stask.is_none() {
            let (tx, rx) = channel(50);
            tokio::spawn(sender_task(rx));
            *stask = Some(tx);
        }
        stask.as_ref().unwrap().clone()
    };
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

/// Validates a bundle entering this node, either received from a peer or
/// submitted by a local application (RFC 9171, 4).
///
/// Rejects malformed bundles as well as bundles without a destination node
/// (dtn:none), which can neither be delivered nor forwarded.
pub fn validate_bundle(bndl: &Bundle) -> Result<()> {
    if bndl.primary.destination.node().is_none() {
        bail!("bundle {} has no valid destination", bndl.id());
    }
    if let Err(errors) = bndl.validate() {
        let (missing_crc, errors): (Vec<_>, Vec<_>) =
            errors.into_iter().partition(is_missing_primary_crc);
        if !errors.is_empty() {
            bail!("bundle {} is invalid: {:?}", bndl.id(), errors);
        }
        if !missing_crc.is_empty() {
            if CONFIG.lock().strict_validation {
                bail!(
                    "bundle {} has no primary block CRC and is not protected by a BIB",
                    bndl.id()
                );
            }
            debug!("Accepting bundle {} without primary block CRC", bndl.id());
        }
    }
    Ok(())
}

/// Bundles without primary block CRC must be protected by a BIB (RFC 9171, 4.3.1).
/// Older dtn7 versions and bp7's defaults create bundles without primary block CRC,
/// so these are only rejected with strict validation.
fn is_missing_primary_crc(err: &bp7::error::Error) -> bool {
    matches!(err, bp7::error::Error::BundleError(msg)
        if msg.starts_with("Primary Block has no CRC"))
}

/// Returns true for the block types this node processes. All other blocks are handled
/// according to their block processing control flags (RFC 9171, 4.2.4).
fn is_known_block_type(block_type: CanonicalBlockType) -> bool {
    match block_type {
        PAYLOAD_BLOCK | PREVIOUS_NODE_BLOCK | BUNDLE_AGE_BLOCK | HOP_COUNT_BLOCK => true,
        // BPSec blocks are only known if this node implements BPSec (RFC 9172, 5)
        #[cfg(feature = "bpsec")]
        bp7::security::INTEGRITY_BLOCK | bp7::security::CONFIDENTIALITY_BLOCK => true,
        _ => false,
    }
}

// handle received/incoming bundles.
pub async fn receive(mut bndl: Bundle) -> Result<()> {
    if let Err(err) = validate_bundle(&bndl) {
        STATS.lock().broken += 1;
        return Err(err);
    }
    if store_add_bundle_if_unknown(&bndl)? {
        info!("Received new bundle: {}", bndl.id());
        STATS.lock().incoming += 1;
    } else {
        debug!(
            "Received an already known bundle, skip processing: {}",
            bndl.id()
        );
        STATS.lock().dups += 1;

        // bundleDeletion is _not_ called because this would delete the already
        // stored BundlePack.
        return Ok(());
    }

    let mut bp = BundlePack::from(&bndl);
    bp.add_constraint(Constraint::DispatchPending);
    bp.sync()?;

    if bndl
        .primary
        .bundle_control_flags
        .contains(BundleControlFlags::BUNDLE_STATUS_REQUEST_RECEPTION)
        && !bndl.is_administrative_record()
        && CONFIG.lock().generate_status_reports
    {
        send_status_report(&bp, RECEIVED_BUNDLE, NO_INFORMATION).await;
    }
    let mut remove_idx = Vec::new();
    for (index, cb) in bndl.canonicals.iter().enumerate() {
        if is_known_block_type(cb.block_type) {
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
            if CONFIG.lock().generate_status_reports {
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
    }
    // Remove canonical blocks marked for deletion, highest index first so that
    // earlier removals do not shift the remaining indices.
    for i in remove_idx.into_iter().rev() {
        bndl.canonicals.remove(i);
    }
    if let Verdict::Drop(reason) = bpsec::enforce(ProcessingStep::Receive, &mut bndl) {
        info!("Bundle violates the security policy: {}", bp.id());
        delete(bp, reason).await?;
        return Ok(());
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

    // the bundle may have been removed concurrently, e.g., by the janitor
    let Some(bndl) = store_get_bundle(bp.id()) else {
        bail!("bundle not found: {}", bp.id());
    };
    if let Err(err) = routing_notify(RoutingNotification::IncomingBundle(bndl)).await {
        error!("Error while sending incoming bundle notification: {}", err);
    }

    if (*DTNCORE.lock()).is_in_endpoints(&bp.destination)
    // TODO: lookup here AND in local delivery, optimize for just one
    {
        trace!(
            "Destination for {} is local endpoint → local_delivery",
            bp.id()
        );
        local_delivery(bp.clone()).await?;
    }
    if !is_local_node_id(&bp.destination) {
        trace!(
            "Destination for {} is NOT local endpoint → forward",
            bp.id()
        );
        tokio::spawn(forward(bp));
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
                bid, hc_limit, hc_count
            );
            if hc.hop_count_exceeded() {
                warn!(
                    "Bundle contains an exceeded hop count block: {} {} {}",
                    bid, hc_limit, hc_count
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
            "Dropping bundle, primary block lifetime is exceeded: {} {:?}",
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
            let offset = (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis() as u64)
                .saturating_sub(bp.received_time);
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
        // bundle age is given in milliseconds (RFC 9171, 4.4.2)
        if std::time::Duration::from_millis(age) >= bundle.primary.lifetime {
            warn!("Dropping bundle, age exceeds lifetime: {}", bundle.id());
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
        let local_eid = CONFIG.lock().host_eid.clone();
        pnb.previous_node_update(local_eid.clone());
        debug!(
            "Previous Node Block was updated: {} {} {}",
            bundle.id(),
            prev_eid,
            local_eid
        );
    } else {
        // according to rfc always add a previous node block
        let local_eid = CONFIG.lock().host_eid.clone();
        let pnb = bp7::canonical::new_previous_node_block(0, BlockControlFlags::empty(), local_eid);
        bundle.add_canonical_block(pnb);
    }
    Ok(bundle)
}
// forward a bundle pack's bundle to another node.
pub async fn forward(mut bp: BundlePack) -> Result<()> {
    let bpid = bp.id().to_string();

    trace!("Forward request for bundle: {}", bpid);
    let Some(_forwarding) = ForwardingGuard::acquire(&bpid) else {
        debug!("Bundle {} is already being forwarded", bpid);
        return Ok(());
    };

    bp.add_constraint(Constraint::ForwardPending);
    bp.remove_constraint(Constraint::DispatchPending);
    trace!("updating bundle info in store: {}", bpid);
    bp.sync()?;

    let bundle_sent = Arc::new(AtomicBool::new(false));

    trace!("Check delivery");

    let (nodes, delete_afterwards) = routing_sender_for_bundle(bp.clone()).await?;
    if !nodes.is_empty() {
        debug!("Attempting forwarding of {} to nodes: {:?}", bp.id(), nodes);
    }

    if nodes.is_empty() {
        trace!("No new peers for forwarding of bundle {}", bp.id());
        if delete_afterwards {
            store_remove(&bpid)?;
        }
    } else {
        debug!("Attempting forwarding of {} to nodes: {:?}", bp.id(), nodes);

        let bndl = store_get_bundle(&bpid);
        if bndl.is_none() {
            bail!("bundle not found: {}", bpid);
        }

        let mut bndl = bndl.unwrap();

        trace!("Handle lifetime");
        handle_primary_lifetime(&bndl).await?;

        trace!("Handle hop count block");
        bndl = handle_hop_count_block(bndl).await?;

        trace!("Handle previous node block");
        // Handle previous node block
        bndl = handle_previous_node_block(bndl).await?;
        trace!("Handle bundle age block");
        // Handle bundle age block
        bndl = handle_bundle_age_block(bndl).await?;

        // only the copy sent is changed, retries start from the stored bundle
        if let Verdict::Drop(reason) = bpsec::enforce(ProcessingStep::Forward, &mut bndl) {
            delete(bp, reason).await?;
            bail!("bundle {} violates the security policy", bpid);
        }

        let mut wg = Vec::new();
        let bundle_data = bndl.to_cbor();
        for n in nodes {
            let bd = bundle_data.clone(); // TODO: optimize cloning away, reference should do
            let bpid = bpid.clone();
            let bundle_sent = std::sync::Arc::clone(&bundle_sent);
            let n = n.clone();
            let task_handle = tokio::spawn(async move {
                let start_time = Instant::now();
                debug!(
                    "Sending bundle to a CLA: {} {} {}",
                    bpid, n.dest, n.cla_name
                );
                if let Err(err) = n.transfer(bd).await {
                    info!(
                        "Sending bundle {} via {} to {} ({}) failed after {:?}",
                        bpid,
                        n.cla_name,
                        n.dest,
                        n.next_hop,
                        start_time.elapsed()
                    );
                    STATS.lock().failed += 1;
                    debug!("Error while transferring bundle {}: {}", bpid, err);

                    let next_hop = n.next_hop.node().unwrap_or_default();
                    if let Err(err) =
                        routing_notify(RoutingNotification::SendingFailed(bpid, next_hop.clone()))
                            .await
                    {
                        error!("Error while sending failed notification: {}", err);
                    }
                    if let Some(dropped_peer) = report_failed_transmission(&next_hop) {
                        if let Err(err) =
                            routing_notify(RoutingNotification::DroppedPeer(dropped_peer.eid)).await
                        {
                            error!("Error while sending dropped peer notification: {}", err);
                        }
                    }
                    // TODO: send status report?
                    // if (*CONFIG.lock()).generate_service_reports {
                    //    send_status_report(&bp2, FORWARDED_BUNDLE, TRANSMISSION_CANCELED);
                    // }
                } else {
                    info!(
                        "Bundle send success: id={} dest={} cla={} in {:?}",
                        bpid,
                        n.dest,
                        n.cla_name,
                        start_time.elapsed()
                    );
                    STATS.lock().outgoing += 1;
                    bundle_sent.store(true, Ordering::Relaxed);
                    if let Err(err) = routing_notify(RoutingNotification::SendingSucceeded(
                        bpid,
                        n.next_hop.node().unwrap_or_default(),
                    ))
                    .await
                    {
                        error!("Error while sending succeeded notification: {}", err);
                    }
                }
            });
            wg.push(task_handle);
        }
        use futures::future::join_all;

        join_all(wg).await;

        // Reset hop count block
        /*if let Some(hc) = bndl.extension_block_by_type_mut(bp7::canonical::HOP_COUNT_BLOCK) {
            if let Some((hc_limit, mut hc_count)) = hc.hop_count_get() {
                hc_count -= 1;
                hc.set_data(bp7::canonical::CanonicalData::HopCount(hc_limit, hc_count));
                debug!(
                    "Bundle's hop count block was reset: {} {} {}",
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
                && CONFIG.lock().generate_status_reports
            {
                send_status_report(&bp, FORWARDED_BUNDLE, NO_INFORMATION).await;
            }
            if delete_afterwards {
                store_remove(&bpid)?;
            } else if bndl.is_administrative_record() {
                // TODO: always inspect all bundles, should be configurable
                is_administrative_record_valid(&bndl);
                contraindicated(bp)?;
            }
        } else {
            info!("Failed to forward bundle to any CLA: {}", bp.id());
            // don't contraindicate if we failed to forward to any CLA
            // retry next janitor run
            //contraindicated(bp)?;
        }
    }
    Ok(())
}

/// Counts a failed transmission to the given peer. Dynamic peers failing too often
/// are removed from the list of known peers and returned.
fn report_failed_transmission(node_name: &str) -> Option<DtnPeer> {
    let mut peers = PEERS.lock();
    let peer = peers.get_mut(node_name)?;
    debug!("Reporting failed sending to peer: {}", node_name);
    peer.report_fail();
    if peer.failed_too_much() && peer.con_type == PeerType::Dynamic {
        debug!(
            "Removing peer {} from list of neighbors due to too many failed transmissions",
            node_name
        );
        peers.remove(node_name)
    } else {
        None
    }
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
    if is_local_node_id(&bp.destination) {
        if bndl
            .primary
            .bundle_control_flags
            .contains(BundleControlFlags::BUNDLE_STATUS_REQUEST_DELIVERY)
            && !bndl.is_administrative_record()
            && CONFIG.lock().generate_status_reports
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
    let delivered = if let Some(aa) = (*DTNCORE.lock()).get_endpoint_mut(&bp.destination) {
        info!("Delivering {}", bp.id());
        aa.push(&bndl);
        true
    } else {
        false
    };
    // STATS must not be locked while holding DTNCORE (lock order: STATS -> DTNCORE)
    if delivered {
        STATS.lock().delivered += 1;
    }
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
    STATS.lock().node.error_info.discarded_bundle_count += 1;
    let bndl = bndl.unwrap();
    if bndl
        .primary
        .bundle_control_flags
        .contains(BundleControlFlags::BUNDLE_STATUS_REQUEST_DELETION)
        && !bndl.is_administrative_record()
        && CONFIG.lock().generate_status_reports
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
                    if store_remove(&bsr.refbundle()).is_err() {
                        warn!(
                            "Status Report could not remove bundle: {} {}",
                            bid,
                            bsr.refbundle()
                        );
                    }
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
    let Some(bndl) = store_get_bundle(bp.id()) else {
        warn!("bundle not found when sending status report: {}", bp.id());
        return;
    };
    // Don't respond to other administrative records or anonymous bundles. The bundle
    // itself is checked, as not every store keeps the administrative flag in its metadata.
    if bndl.is_administrative_record() || bndl.primary.source == EndpointID::none() {
        warn!("status report sending denied for dtn:none sources/administrative bundles themselves: {}", bp.id());
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
        CONFIG.lock().host_eid.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use bp7::canonical::{new_bundle_age_block, new_payload_block};
    use bp7::primary::PrimaryBlockBuilder;
    use std::convert::TryFrom;
    use std::time::Duration;

    fn bundle_with_age(age_ms: u64, lifetime: Duration) -> Bundle {
        let src = EndpointID::try_from("dtn://age-sender/app".to_string()).unwrap();
        let dst = EndpointID::try_from("dtn://age-receiver/app".to_string()).unwrap();
        let primary = PrimaryBlockBuilder::default()
            .destination(dst)
            .source(src.clone())
            .report_to(src)
            .creation_timestamp(bp7::CreationTimestamp::now())
            .lifetime(lifetime)
            .build()
            .unwrap();
        Bundle {
            primary,
            canonicals: vec![
                new_bundle_age_block(2, BlockControlFlags::empty(), age_ms),
                new_payload_block(BlockControlFlags::empty(), b"age".to_vec()),
            ],
        }
    }

    fn add_peer(name: &str, con_type: PeerType, fails: u16) {
        let mut peer = DtnPeer::new(
            EndpointID::try_from(format!("dtn://{}/", name)).unwrap(),
            "127.0.0.1".parse::<std::net::IpAddr>().unwrap().into(),
            con_type,
            None,
            Vec::new(),
            std::collections::HashMap::new(),
        );
        peer.fails = fails;
        crate::peers_add(peer);
    }

    #[test]
    fn dynamic_peers_failing_too_often_are_dropped() {
        add_peer("failing-dynamic", PeerType::Dynamic, 3);
        let dropped = report_failed_transmission("failing-dynamic").expect("peer not dropped");
        assert_eq!(dropped.node_name(), "failing-dynamic");
        assert!(!crate::peers_known("failing-dynamic"));

        add_peer("failing-static", PeerType::Static, 3);
        assert!(report_failed_transmission("failing-static").is_none());
        assert!(crate::peers_known("failing-static"));

        assert!(report_failed_transmission("unknown-peer").is_none());
    }

    #[test]
    fn bundle_is_not_forwarded_concurrently() {
        let first = ForwardingGuard::acquire("dtn://guard-test/-1-0");
        assert!(first.is_some());
        assert!(ForwardingGuard::acquire("dtn://guard-test/-1-0").is_none());
        assert!(ForwardingGuard::acquire("dtn://guard-test/-2-0").is_some());

        drop(first);
        assert!(ForwardingGuard::acquire("dtn://guard-test/-1-0").is_some());
    }

    #[tokio::test]
    async fn bundle_age_is_interpreted_as_milliseconds() {
        // 5000 ms of age exceed a lifetime of 3 s
        let bndl = bundle_with_age(5000, Duration::from_secs(3));
        store_push_bundle(&bndl).unwrap();
        assert!(handle_bundle_age_block(bndl).await.is_err());

        // 1000 ms of age are within a lifetime of 3 s
        let bndl = bundle_with_age(1000, Duration::from_secs(3));
        store_push_bundle(&bndl).unwrap();
        let mut bndl = handle_bundle_age_block(bndl).await.unwrap();
        match bndl
            .extension_block_by_type_mut(BUNDLE_AGE_BLOCK)
            .unwrap()
            .data()
        {
            CanonicalData::BundleAge(age) => assert!(*age >= 1000 && *age < 3000),
            other => panic!("unexpected bundle age data: {:?}", other),
        }
    }
}
