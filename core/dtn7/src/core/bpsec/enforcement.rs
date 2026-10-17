//! Enforcement: carries out the security actions demanded by the policy with the
//! security operations of bp7 and handles failures as the demanding rules configure.

use super::config::{
    AesVariant, FailureAction, SecurityContext, SecurityOperation, SecurityRule, ShaVariant,
};
use super::keys::KeySet;
use super::policy::{self, ActionKind, SecurityAction};
use super::statistics::{Event, Statistics};
use super::{ProcessingStep, Verdict};
use bp7::administrative_record::{
    StatusReportReason, FAILED_SECURITY_OPERATION, MISSING_SECURITY_OPERATION,
};
use bp7::bpsec::{self, BcbSecurityContextParameter, BibSecurityContextParameter, BpsecError};
use bp7::{Bundle, EndpointID};
use log::{debug, warn};
use rand::rngs::OsRng;
use rand::RngCore;
use std::fmt;
use zeroize::Zeroizing;

/// CRC type of security targets whose BIB was accepted or BCB decrypted (RFC 9173, 3.8.2,
/// 4.8.2)
const ACCEPTED_TARGET_CRC: bp7::crc::CrcRawType = bp7::crc::CRC_32;

/// Applies the security rules to a bundle at a processing step. The security operations
/// change the bundle in place, e.g., by adding or removing BIBs and BCBs. If an operation
/// fails, the failure action of its rule decides whether the bundle is dropped.
///
/// `node` is the endpoint ID of this node, used as security source of new security
/// blocks. What the rules did is recorded in `statistics`.
pub fn enforce(
    rules: &[SecurityRule],
    keys: &KeySet,
    node: &EndpointID,
    step: ProcessingStep,
    bundle: &mut Bundle,
    statistics: &mut Statistics,
) -> Verdict {
    let mut actions = policy::evaluate(rules, step, bundle);
    if actions.is_empty() {
        return Verdict::Process;
    }
    if actions.iter().any(is_decryption) {
        // BIBs encrypted by a BCB only become visible after decrypting, so the BCB
        // actions come first, then the policy is evaluated again for the BIB actions.
        // Failed decryptions are thus not tried again.
        for action in actions.iter().filter(|action| is_bcb_action(action)) {
            if let Err(verdict) = carry_out(action, keys, node, bundle, statistics) {
                return verdict;
            }
        }
        actions = policy::evaluate(rules, step, bundle);
        actions.retain(|action| !is_bcb_action(action));
    }
    for action in &actions {
        if let Err(verdict) = carry_out(action, keys, node, bundle, statistics) {
            return verdict;
        }
    }
    // bp7 only sets the CRC types of changed blocks, their CRCs are calculated here to
    // leave a valid bundle
    bundle.calculate_crc();
    Verdict::Process
}

fn is_decryption(action: &SecurityAction) -> bool {
    matches!(action.kind, ActionKind::Decrypt { .. })
}

fn is_bcb_action(action: &SecurityAction) -> bool {
    action.rule.operation == SecurityOperation::Bcb
}

/// Carries out a security action and handles its failure. Returns the verdict if the
/// bundle is dropped.
fn carry_out(
    action: &SecurityAction,
    keys: &KeySet,
    node: &EndpointID,
    bundle: &mut Bundle,
    statistics: &mut Statistics,
) -> Result<(), Verdict> {
    let rule = action.rule;
    let failure = match apply(action, keys, node, bundle) {
        Ok(blocks) => {
            if let Some(event) = success_event(&action.kind) {
                statistics.record_n(&rule.name, event, blocks);
            }
            debug!(
                "BPSec rule {}: {:?} succeeded for bundle {}",
                rule.name,
                action.kind,
                bundle.id()
            );
            return Ok(());
        }
        Err(failure) => failure,
    };
    warn!(
        "BPSec rule {}: {:?} failed for bundle {}: {}",
        rule.name,
        action.kind,
        bundle.id(),
        failure
    );
    statistics.record(&rule.name, failure.event());
    match rule.on_failure {
        FailureAction::Drop => {
            statistics.record(&rule.name, Event::Dropped);
            return Err(Verdict::Drop(failure.reason()));
        }
        FailureAction::RemoveOperation => remove_operation(&action.kind, bundle),
        FailureAction::Log => {}
    }
    Ok(())
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
    fn event(&self) -> Event {
        match self {
            Failure::Missing => Event::Missing,
            _ => Event::Failed,
        }
    }

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

/// Carries out a security action, returning the number of security blocks added,
/// checked or removed.
fn apply(
    action: &SecurityAction,
    keys: &KeySet,
    node: &EndpointID,
    bundle: &mut Bundle,
) -> Result<u64, Failure> {
    let rule = action.rule;
    let verified = |verified| {
        if verified {
            Ok(1)
        } else {
            Err(Failure::NotVerified)
        }
    };
    match action.kind {
        ActionKind::Missing { .. } => Err(Failure::Missing),
        ActionKind::Sign { target } => {
            let parameters = bib_parameters(&rule.context);
            bpsec::sign(
                bundle,
                &[target],
                key(keys, rule)?,
                node.clone(),
                parameters,
            )?;
            Ok(1)
        }
        ActionKind::Verify { bib } => verified(bpsec::verify(bundle, bib, key(keys, rule)?)?),
        ActionKind::Accept { bib } => verified(bpsec::accept(
            bundle,
            bib,
            key(keys, rule)?,
            ACCEPTED_TARGET_CRC,
        )?),
        ActionKind::Encrypt { target } => {
            Ok(encrypt(bundle, target, rule, key(keys, rule)?, node)?)
        }
        ActionKind::Decrypt { bcb } => {
            bpsec::decrypt(bundle, bcb, key(keys, rule)?, ACCEPTED_TARGET_CRC)?;
            Ok(1)
        }
    }
}

fn success_event(kind: &ActionKind) -> Option<Event> {
    match kind {
        ActionKind::Sign { .. } => Some(Event::Signed),
        ActionKind::Verify { .. } => Some(Event::Verified),
        ActionKind::Accept { .. } => Some(Event::Accepted),
        ActionKind::Encrypt { .. } => Some(Event::Encrypted),
        ActionKind::Decrypt { .. } => Some(Event::Decrypted),
        ActionKind::Missing { .. } => None,
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

/// Encrypts a target with a BCB, after encrypting the BIBs protecting it, as BIBs have to
/// be encrypted along with their targets (RFC 9172, 3.9). Each block gets its own BCB with
/// a new random IV, as reusing an IV with the same key breaks AES-GCM (RFC 9173, 4.6).
/// The bundle is only changed if all encryptions succeed. Returns the number of BCBs added.
fn encrypt(
    bundle: &mut Bundle,
    target: u64,
    rule: &SecurityRule,
    key: &[u8],
    node: &EndpointID,
) -> Result<u64, BpsecError> {
    let bibs = policy::bibs_of(bundle, target);
    if bibs.is_empty() {
        encrypt_block(bundle, target, rule, key, node)?;
        return Ok(1);
    }
    // a copy, so a failure leaves no BIB encrypted without its target
    let mut encrypted = bundle.clone();
    let blocks: Vec<u64> = bibs.into_iter().chain([target]).collect();
    for block in &blocks {
        encrypt_block(&mut encrypted, *block, rule, key, node)?;
    }
    *bundle = encrypted;
    Ok(blocks.len() as u64)
}

fn encrypt_block(
    bundle: &mut Bundle,
    block: u64,
    rule: &SecurityRule,
    key: &[u8],
    node: &EndpointID,
) -> Result<(), BpsecError> {
    let variant = rule.aes_variant();
    let mut parameters = BcbSecurityContextParameter {
        aes_variant: Some(match variant {
            AesVariant::Aes128 => bpsec::AES_128_GCM,
            AesVariant::Aes256 => bpsec::AES_256_GCM,
        }),
        ..Default::default()
    };
    // with a wrapped key, the configured key is the key encryption key
    if rule.context.wrapped_key {
        let content_key = Zeroizing::new(random_bytes(variant.key_len()));
        parameters.wrapped_key = Some(bpsec::wrap_key(key, &content_key)?);
    }
    let iv = random_bytes(bpsec::IV_LEN);
    bpsec::encrypt(bundle, block, key, &iv, node.clone(), parameters)?;
    Ok(())
}

/// Random bytes from the random number generator of the operating system
fn random_bytes(len: usize) -> Vec<u8> {
    let mut bytes = vec![0; len];
    OsRng.fill_bytes(&mut bytes);
    bytes
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
        // missing operations leave nothing to remove, failed sources only drop or log and
        // BCBs can not be removed without decrypting them
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::bpsec::config::{SecurityRole, SecurityTarget};
    use std::collections::HashSet;
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

    fn key_set(files: &[(&str, &str)]) -> KeySet {
        let dir = tempfile::tempdir().unwrap();
        for (id, key) in files {
            std::fs::write(dir.path().join(format!("{}.key", id)), key).unwrap();
        }
        KeySet::load(dir.path()).unwrap()
    }

    /// The given HMAC key and the AES keys of the BCB rules
    fn keys(hmac: &str) -> KeySet {
        key_set(&[("hmac", hmac), ("aes", AES_KEY), ("kek", KEK)])
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

    /// A BCB rule for the payload, with a wrapped key for the key "kek"
    fn bcb_rule(role: SecurityRole, key: &str, on_failure: FailureAction) -> SecurityRule {
        let mut rule = rule(role, &[SecurityTarget::Payload], on_failure);
        rule.name = format!("{:?}-bcb", role);
        rule.operation = SecurityOperation::Bcb;
        rule.key = key.into();
        rule.context.wrapped_key = key == "kek";
        rule
    }

    fn bib_count(bundle: &Bundle) -> usize {
        bundle.decode_bibs().len()
    }

    const KEY: &str = "00112233445566778899aabbccddeeff";
    const AES_KEY: &str = "2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b";
    const KEK: &str = "3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c";
    const PLAINTEXT: &[u8] = b"temperature: 21C";

    /// Signs both targets and encrypts the payload at node1, with the given BCB key.
    fn encrypted(bcb_key: &str) -> Bundle {
        let rules = [
            rule(
                SecurityRole::Source,
                &[SecurityTarget::Payload, SecurityTarget::Primary],
                FailureAction::Drop,
            ),
            bcb_rule(SecurityRole::Source, bcb_key, FailureAction::Drop),
        ];
        let mut bundle = bundle();
        let mut statistics = Statistics::default();
        let verdict = enforce(
            &rules,
            &keys(KEY),
            &eid("dtn://node1/"),
            Transmit,
            &mut bundle,
            &mut statistics,
        );
        assert_eq!(verdict, Verdict::Process);
        // one BCB for the payload, one for its BIB
        assert_eq!(statistics.rule("Source-bcb").encrypted, 2);
        // the bundle survives encoding, as on its way to the next node
        Bundle::try_from(bundle.to_cbor()).unwrap()
    }

    /// Decrypts the payload with the given key and accepts both BIBs at node2.
    fn decrypt(bcb_key: &str, keys: &KeySet, bundle: &mut Bundle) -> (Verdict, Statistics) {
        let rules = [
            bcb_rule(SecurityRole::Acceptor, bcb_key, FailureAction::Drop),
            rule(
                SecurityRole::Acceptor,
                &[SecurityTarget::Payload, SecurityTarget::Primary],
                FailureAction::Drop,
            ),
        ];
        let mut statistics = Statistics::default();
        let verdict = enforce(
            &rules,
            keys,
            &eid("dtn://node2/"),
            Receive,
            bundle,
            &mut statistics,
        );
        (verdict, statistics)
    }

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
            &mut Statistics::default(),
        );
        assert_eq!(verdict, Verdict::Process);
        bundle
    }

    fn receive(rules: &[SecurityRule], keys: &KeySet, bundle: &mut Bundle) -> Verdict {
        let mut statistics = Statistics::default();
        enforce(
            rules,
            keys,
            &eid("dtn://node2/"),
            Receive,
            bundle,
            &mut statistics,
        )
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
            "only the failed BIB of the payload is removed"
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
            &mut Statistics::default(),
        );
        assert_eq!(verdict, Verdict::Drop(FAILED_SECURITY_OPERATION));
    }

    #[test]
    fn operations_are_counted_per_rule() {
        let targets = [SecurityTarget::Payload, SecurityTarget::Primary];
        let rules = [
            rule(SecurityRole::Acceptor, &targets, FailureAction::Drop),
            rule(SecurityRole::Verifier, &targets, FailureAction::Log),
        ];
        let mut statistics = Statistics::default();
        let mut receive = |bundle: &mut Bundle| {
            enforce(
                &rules,
                &keys(KEY),
                &eid("dtn://node2/"),
                Receive,
                bundle,
                &mut statistics,
            )
        };
        receive(&mut signed());
        let mut tampered = signed();
        tampered.primary.lifetime = std::time::Duration::from_secs(1);
        receive(&mut tampered);
        receive(&mut bundle());

        // the acceptor handles all BIBs as the first matching rule
        let acceptor = statistics.rule("Acceptor");
        // the tampered primary block is covered by both BIBs, the first failure drops
        // the bundle, as does the first missing BIB of the unsigned bundle
        assert_eq!(acceptor.accepted, 2);
        assert_eq!((acceptor.failed, acceptor.missing), (1, 1));
        assert_eq!(acceptor.dropped, 2);
        assert_eq!(statistics.rule("Verifier"), Default::default());
    }

    #[test]
    fn encrypted_bundles_are_decrypted_and_accepted() {
        let mut bundle = encrypted("aes");
        // the BIB of the payload is encrypted along with it, each with its own BCB
        assert_eq!(bundle.decode_bcbs().len(), 2);
        assert_eq!(bib_count(&bundle), 1);
        assert_ne!(bundle.payload().unwrap().as_slice(), PLAINTEXT);

        let (verdict, statistics) = decrypt("aes", &keys(KEY), &mut bundle);
        assert_eq!(verdict, Verdict::Process);
        assert_eq!(bundle.payload().unwrap().as_slice(), PLAINTEXT);
        assert!(bundle.decode_bcbs().is_empty());
        assert_eq!(bib_count(&bundle), 0, "the decrypted BIB was not accepted");
        bundle.validate().unwrap();
        assert_eq!(statistics.rule("Acceptor-bcb").decrypted, 2);
        assert_eq!(statistics.rule("Acceptor").accepted, 2);
    }

    #[test]
    fn content_keys_are_wrapped() {
        let bundle = encrypted("kek");
        for bcb in bundle.decode_bcbs() {
            let parameters = bcb
                .parameters::<BcbSecurityContextParameter>()
                .unwrap()
                .unwrap();
            assert!(parameters.wrapped_key.is_some());
        }
        assert_eq!(
            decrypt("kek", &keys(KEY), &mut bundle.clone()).0,
            Verdict::Process
        );

        let other_kek = key_set(&[("hmac", KEY), ("kek", &"ff".repeat(16))]);
        assert_eq!(
            decrypt("kek", &other_kek, &mut bundle.clone()).0,
            Verdict::Drop(FAILED_SECURITY_OPERATION)
        );
    }

    #[test]
    fn ivs_are_never_reused() {
        let mut ivs = HashSet::new();
        for _ in 0..50 {
            for bcb in encrypted("aes").decode_bcbs() {
                let iv = bcb
                    .parameters::<BcbSecurityContextParameter>()
                    .unwrap()
                    .and_then(|parameters| parameters.iv)
                    .unwrap();
                assert_eq!(iv.len(), bpsec::IV_LEN);
                assert!(ivs.insert(iv), "IV used twice");
            }
        }
    }

    #[test]
    fn failed_decryptions_are_not_retried() {
        let mut tampered = encrypted("aes");
        let payload = tampered
            .extension_block_by_type_mut(bp7::PAYLOAD_BLOCK)
            .unwrap();
        let mut ciphertext = match payload.data() {
            bp7::CanonicalData::Data(data) => data.clone(),
            other => panic!("unexpected payload data: {:?}", other),
        };
        ciphertext[0] ^= 1;
        payload.set_data(bp7::CanonicalData::Data(ciphertext));
        assert_eq!(
            decrypt("aes", &keys(KEY), &mut tampered.clone()).0,
            Verdict::Drop(FAILED_SECURITY_OPERATION)
        );

        // logged failures leave the payload encrypted, the decrypted BIB of the primary
        // block is accepted nevertheless
        let rules = [
            bcb_rule(SecurityRole::Acceptor, "aes", FailureAction::Log),
            rule(
                SecurityRole::Acceptor,
                &[SecurityTarget::Primary],
                FailureAction::Drop,
            ),
        ];
        let mut statistics = Statistics::default();
        let verdict = enforce(
            &rules,
            &keys(KEY),
            &eid("dtn://node2/"),
            Receive,
            &mut tampered,
            &mut statistics,
        );
        assert_eq!(verdict, Verdict::Process);
        let bcb_statistics = statistics.rule("Acceptor-bcb");
        assert_eq!((bcb_statistics.decrypted, bcb_statistics.failed), (1, 1));
        assert_eq!(statistics.rule("Acceptor").accepted, 1);
        assert_eq!(tampered.decode_bcbs().len(), 1);
    }
}
