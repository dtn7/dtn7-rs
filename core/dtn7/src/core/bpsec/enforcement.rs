//! Enforcement: carries out the security actions demanded by the policy with the
//! security operations of bp7 and handles failures as the demanding rules configure.

use super::config::{FailureAction, SecurityContext, SecurityRule, ShaVariant};
use super::keys::KeySet;
use super::policy::{self, ActionKind, SecurityAction};
use super::{ProcessingStep, Verdict};
use bp7::administrative_record::{
    StatusReportReason, FAILED_SECURITY_OPERATION, MISSING_SECURITY_OPERATION,
};
use bp7::bpsec::{self, BibSecurityContextParameter, BpsecError};
use bp7::{Bundle, EndpointID};
use log::{debug, warn};
use std::fmt;

/// CRC type of security targets whose BIB was accepted (RFC 9173, 3.8.2)
const ACCEPTED_TARGET_CRC: bp7::crc::CrcRawType = bp7::crc::CRC_32;

/// Applies the security rules to a bundle at a processing step. The security operations
/// change the bundle in place, e.g., by adding or removing BIBs. If an operation fails,
/// the failure action of its rule decides whether the bundle is dropped.
///
/// `node` is the endpoint ID of this node, used as security source of new BIBs.
pub fn enforce(
    rules: &[SecurityRule],
    keys: &KeySet,
    node: &EndpointID,
    step: ProcessingStep,
    bundle: &mut Bundle,
) -> Verdict {
    let actions = policy::evaluate(rules, step, bundle);
    if actions.is_empty() {
        return Verdict::Process;
    }
    for action in actions {
        let rule = action.rule;
        let Err(failure) = apply(&action, keys, node, bundle) else {
            debug!(
                "BPSec rule {}: {:?} succeeded for bundle {}",
                rule.name,
                action.kind,
                bundle.id()
            );
            continue;
        };
        warn!(
            "BPSec rule {}: {:?} failed for bundle {}: {}",
            rule.name,
            action.kind,
            bundle.id(),
            failure
        );
        match rule.on_failure {
            FailureAction::Drop => return Verdict::Drop(failure.reason()),
            FailureAction::RemoveOperation => remove_operation(&action.kind, bundle),
            FailureAction::Log => {}
        }
    }
    // bp7 only sets the CRC types of changed blocks, their CRCs are calculated here to
    // leave a valid bundle
    bundle.calculate_crc();
    Verdict::Process
}

/// Why a security action failed
#[derive(Debug)]
enum Failure {
    /// The rule demands a security operation the bundle lacks
    Missing,
    /// The security result of a BIB does not match its targets
    NotVerified,
    /// The key of the rule is not loaded
    UnknownKey(String),
    /// bp7 rejected the security operation
    Bpsec(BpsecError),
}

impl Failure {
    /// Status report reason for bundles dropped due to this failure (RFC 9172, 7.1)
    fn reason(&self) -> StatusReportReason {
        match self {
            Failure::Missing => MISSING_SECURITY_OPERATION,
            Failure::Bpsec(err) => err.status_report_reason(),
            Failure::NotVerified | Failure::UnknownKey(_) => FAILED_SECURITY_OPERATION,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Missing => write!(f, "security operation missing"),
            Failure::NotVerified => write!(f, "integrity check failed"),
            Failure::UnknownKey(id) => write!(f, "key {} is not loaded", id),
            Failure::Bpsec(err) => write!(f, "{}", err),
        }
    }
}

impl From<BpsecError> for Failure {
    fn from(err: BpsecError) -> Self {
        Failure::Bpsec(err)
    }
}

fn apply(
    action: &SecurityAction,
    keys: &KeySet,
    node: &EndpointID,
    bundle: &mut Bundle,
) -> Result<(), Failure> {
    let rule = action.rule;
    let verified = match action.kind {
        ActionKind::Missing { .. } => return Err(Failure::Missing),
        ActionKind::Sign { target } => {
            let parameters = bib_parameters(&rule.context);
            bpsec::sign(
                bundle,
                &[target],
                key(keys, rule)?,
                node.clone(),
                parameters,
            )?;
            true
        }
        ActionKind::Verify { bib } => bpsec::verify(bundle, bib, key(keys, rule)?)?,
        ActionKind::Accept { bib } => {
            bpsec::accept(bundle, bib, key(keys, rule)?, ACCEPTED_TARGET_CRC)?
        }
        // the configuration rejects BCB rules as long as BCBs are not supported
        ActionKind::Encrypt { .. } | ActionKind::Decrypt { .. } => {
            return Err(BpsecError::Unsupported("BCB rules".to_string()).into())
        }
    };
    if verified {
        Ok(())
    } else {
        Err(Failure::NotVerified)
    }
}

fn key<'k>(keys: &'k KeySet, rule: &SecurityRule) -> Result<&'k [u8], Failure> {
    keys.get(&rule.key)
        .map(|key| key.bytes())
        .ok_or_else(|| Failure::UnknownKey(rule.key.clone()))
}

fn bib_parameters(context: &SecurityContext) -> BibSecurityContextParameter {
    let variant = match context.sha_variant.unwrap_or(ShaVariant::Sha384) {
        ShaVariant::Sha256 => bpsec::HMAC_SHA_256,
        ShaVariant::Sha384 => bpsec::HMAC_SHA_384,
        ShaVariant::Sha512 => bpsec::HMAC_SHA_512,
    };
    BibSecurityContextParameter {
        sha_variant: Some((bpsec::SHA_VARIANT_PARAMETER as u8, variant)),
        ..Default::default()
    }
}

/// Removes a failed security operation from the bundle, keeping its targets
/// (RFC 9172, 5.1.1).
fn remove_operation(kind: &ActionKind, bundle: &mut Bundle) {
    match kind {
        ActionKind::Verify { bib } | ActionKind::Accept { bib } => {
            if let Err(err) = bpsec::remove_block(bundle, *bib) {
                warn!(
                    "Could not remove BIB {} of bundle {}: {}",
                    bib,
                    bundle.id(),
                    err
                );
            }
        }
        // missing operations leave nothing to remove, failed sources only drop or log
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::bpsec::config::{SecurityOperation, SecurityRole, SecurityTarget};
    use std::convert::TryFrom;
    use ProcessingStep::*;

    fn eid(s: &str) -> EndpointID {
        EndpointID::try_from(s.to_string()).unwrap()
    }

    fn bundle() -> Bundle {
        let mut bundle = bp7::bundle::new_std_payload_bundle(
            eid("dtn://node1/sensor"),
            eid("dtn://node2/sensors"),
            b"temperature: 21C".to_vec(),
        );
        bundle.set_crc(bp7::crc::CRC_32);
        bundle
    }

    fn keys(hmac: &str) -> KeySet {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hmac.key"), hmac).unwrap();
        KeySet::load(dir.path()).unwrap()
    }

    fn rule(
        role: SecurityRole,
        targets: &[SecurityTarget],
        on_failure: FailureAction,
    ) -> SecurityRule {
        SecurityRule {
            name: format!("{:?}", role),
            role,
            source: "**".into(),
            destination: "**".into(),
            operation: SecurityOperation::Bib,
            targets: targets.to_vec(),
            key: "hmac".into(),
            context: SecurityContext::default(),
            on_failure,
        }
    }

    fn bib_count(bundle: &Bundle) -> usize {
        bundle.decode_bibs().len()
    }

    const KEY: &str = "00112233445566778899aabbccddeeff";

    /// Signs a bundle for both targets at node1, as a security source would.
    fn signed() -> Bundle {
        let sign = [rule(
            SecurityRole::Source,
            &[SecurityTarget::Payload, SecurityTarget::Primary],
            FailureAction::Drop,
        )];
        let mut bundle = bundle();
        let verdict = enforce(
            &sign,
            &keys(KEY),
            &eid("dtn://node1/"),
            Transmit,
            &mut bundle,
        );
        assert_eq!(verdict, Verdict::Process);
        bundle
    }

    fn receive(rules: &[SecurityRule], keys: &KeySet, bundle: &mut Bundle) -> Verdict {
        enforce(rules, keys, &eid("dtn://node2/"), Receive, bundle)
    }

    #[test]
    fn signed_bundles_are_verified_and_accepted() {
        let targets = [SecurityTarget::Payload, SecurityTarget::Primary];
        let mut bundle = signed();
        assert_eq!(bib_count(&bundle), 2);
        // the bundle survives encoding, as on its way to the next node
        let mut bundle = Bundle::try_from(bundle.to_cbor()).unwrap();

        let verify = [rule(SecurityRole::Verifier, &targets, FailureAction::Drop)];
        assert_eq!(receive(&verify, &keys(KEY), &mut bundle), Verdict::Process);
        assert_eq!(bib_count(&bundle), 2, "a verifier keeps the BIBs");

        let accept = [rule(SecurityRole::Acceptor, &targets, FailureAction::Drop)];
        assert_eq!(receive(&accept, &keys(KEY), &mut bundle), Verdict::Process);
        assert_eq!(bib_count(&bundle), 0, "an acceptor removes the BIBs");
        bundle.validate().unwrap();
    }

    #[test]
    fn tampered_bundles_are_dropped() {
        let mut bundle = signed();
        bundle.primary.lifetime = std::time::Duration::from_secs(1);
        let accept = [rule(
            SecurityRole::Acceptor,
            &[SecurityTarget::Primary],
            FailureAction::Drop,
        )];
        assert_eq!(
            receive(&accept, &keys(KEY), &mut bundle),
            Verdict::Drop(FAILED_SECURITY_OPERATION)
        );

        // as well as bundles signed with another key
        let mut bundle = signed();
        let other_key = "ff".repeat(16);
        assert_eq!(
            receive(&accept, &keys(&other_key), &mut bundle),
            Verdict::Drop(FAILED_SECURITY_OPERATION)
        );
    }

    #[test]
    fn missing_operations_fail() {
        let accept = |on_failure| {
            [rule(
                SecurityRole::Acceptor,
                &[SecurityTarget::Payload],
                on_failure,
            )]
        };
        assert_eq!(
            receive(&accept(FailureAction::Drop), &keys(KEY), &mut bundle()),
            Verdict::Drop(MISSING_SECURITY_OPERATION)
        );
        assert_eq!(
            receive(&accept(FailureAction::Log), &keys(KEY), &mut bundle()),
            Verdict::Process
        );
    }

    #[test]
    fn failed_operations_can_be_removed_or_logged() {
        let other_keys = keys(&"ff".repeat(16));
        let verify = |on_failure| {
            [rule(
                SecurityRole::Verifier,
                &[SecurityTarget::Payload],
                on_failure,
            )]
        };

        let mut bundle = signed();
        let verdict = receive(
            &verify(FailureAction::RemoveOperation),
            &other_keys,
            &mut bundle,
        );
        assert_eq!(verdict, Verdict::Process);
        assert_eq!(
            bib_count(&bundle),
            1,
            "the failed BIB of the payload is kept"
        );

        let mut bundle = signed();
        let verdict = receive(&verify(FailureAction::Log), &other_keys, &mut bundle);
        assert_eq!(verdict, Verdict::Process);
        assert_eq!(bib_count(&bundle), 2);
    }

    #[test]
    fn bundles_without_matching_rules_are_unchanged() {
        let mut accept = rule(
            SecurityRole::Acceptor,
            &[SecurityTarget::Payload],
            FailureAction::Drop,
        );
        accept.source = "dtn://node3/**".into();
        let mut bundle = signed();
        let before = bundle.clone();
        assert_eq!(
            receive(&[accept], &keys(KEY), &mut bundle),
            Verdict::Process
        );
        assert_eq!(bundle, before);
    }

    #[test]
    fn unknown_keys_fail() {
        let mut sign = rule(
            SecurityRole::Source,
            &[SecurityTarget::Payload],
            FailureAction::Drop,
        );
        sign.key = "missing".into();
        let verdict = enforce(
            &[sign],
            &keys(KEY),
            &eid("dtn://node1/"),
            Forward,
            &mut bundle(),
        );
        assert_eq!(verdict, Verdict::Drop(FAILED_SECURITY_OPERATION));
    }
}
