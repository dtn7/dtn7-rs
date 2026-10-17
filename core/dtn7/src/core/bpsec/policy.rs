//! Policy engine: decides which security operations a bundle needs at a processing step.
//!
//! The functions are pure, without keys, I/O or global state. The resulting actions are
//! carried out by the enforcement.

use super::config::{SecurityOperation, SecurityRole, SecurityRule, SecurityTarget};
use bp7::bpsec::{AbstractSecurityBlock, CONFIDENTIALITY_BLOCK, INTEGRITY_BLOCK};
use bp7::{Bundle, CanonicalData};
use glob_match::glob_match;
use std::collections::HashSet;

/// Block number of the primary block as security target (RFC 9172, 3.6)
const PRIMARY_BLOCK_NUMBER: u64 = 0;
/// Block number of the payload block (RFC 9171, 4.3.2)
const PAYLOAD_BLOCK_NUMBER: u64 = 1;

/// Where in the bundle processing the policy is evaluated
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingStep {
    /// A local application sends a bundle
    Transmit,
    /// A bundle was received from a peer
    Receive,
    /// A bundle is forwarded to a peer
    Forward,
}

/// A security operation the policy demands for a bundle
#[derive(Debug, PartialEq, Eq)]
pub struct SecurityAction<'a> {
    /// The rule demanding the action, with its key, context and failure action
    pub rule: &'a SecurityRule,
    pub kind: ActionKind,
}

/// Security targets and blocks are identified by their block numbers
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionKind {
    /// Add a BIB for the target
    Sign { target: u64 },
    /// Encrypt the target with a new BCB, together with the BIBs protecting the target,
    /// which must be encrypted as well (RFC 9172, 3.9)
    Encrypt { target: u64 },
    /// Verify the BIB and keep it
    Verify { bib: u64 },
    /// Verify the BIB and remove it
    Accept { bib: u64 },
    /// Decrypt the targets of the BCB and remove it
    Decrypt { bcb: u64 },
    /// The rule demands a security operation for the target, but the bundle has none
    Missing { target: u64 },
}

/// A security block already present in the bundle
#[derive(Debug)]
struct SecurityBlock {
    number: u64,
    operation: SecurityOperation,
    targets: Vec<u64>,
}

/// Returns the security actions for a bundle at a processing step, in the order they
/// have to be carried out.
///
/// Source rules apply when a bundle leaves this node, verifier and acceptor rules when
/// it is received. A security block is handled by the first matching rule. BIBs
/// encrypted by a BCB only become visible after decrypting, so the policy has to be
/// evaluated again after Decrypt actions.
pub fn evaluate<'a>(
    rules: &'a [SecurityRule],
    step: ProcessingStep,
    bundle: &Bundle,
) -> Vec<SecurityAction<'a>> {
    let matching = rules
        .iter()
        .filter(|rule| applies_at(rule.role, step) && matches_bundle(rule, bundle));
    let existing = security_blocks(bundle);
    match step {
        ProcessingStep::Transmit | ProcessingStep::Forward => source_actions(matching, &existing),
        ProcessingStep::Receive => receiver_actions(matching, &existing),
    }
}

fn applies_at(role: SecurityRole, step: ProcessingStep) -> bool {
    match role {
        SecurityRole::Source => step != ProcessingStep::Receive,
        SecurityRole::Verifier | SecurityRole::Acceptor => step == ProcessingStep::Receive,
    }
}

fn matches_bundle(rule: &SecurityRule, bundle: &Bundle) -> bool {
    glob_match(&rule.source, &bundle.primary.source.to_string())
        && glob_match(&rule.destination, &bundle.primary.destination.to_string())
}

fn target_numbers(rule: &SecurityRule) -> Vec<u64> {
    rule.targets
        .iter()
        .map(|target| match target {
            SecurityTarget::Primary => PRIMARY_BLOCK_NUMBER,
            SecurityTarget::Payload => PAYLOAD_BLOCK_NUMBER,
        })
        .collect()
}

/// Security sources protect each target once: targets already protected by an existing
/// block or an earlier rule are skipped. BIBs are added before BCBs, and encrypted
/// targets are not signed, as a BIB must not target ciphertext (RFC 9172, 3.9).
///
/// Each target gets its own BIB. A BIB has to be encrypted along with its targets, so a
/// BIB for several targets would hide the integrity protection of all of them.
fn source_actions<'a>(
    rules: impl Iterator<Item = &'a SecurityRule>,
    existing: &[SecurityBlock],
) -> Vec<SecurityAction<'a>> {
    let mut signed = targets_of(existing, SecurityOperation::Bib);
    let already_encrypted = targets_of(existing, SecurityOperation::Bcb);
    let mut encrypted = already_encrypted.clone();
    let mut signs = Vec::new();
    let mut encryptions = Vec::new();
    for rule in rules {
        match rule.operation {
            SecurityOperation::Bib => {
                for target in target_numbers(rule) {
                    if !already_encrypted.contains(&target) && signed.insert(target) {
                        signs.push(SecurityAction {
                            rule,
                            kind: ActionKind::Sign { target },
                        });
                    }
                }
            }
            SecurityOperation::Bcb => {
                for target in target_numbers(rule) {
                    if encrypted.insert(target) {
                        encryptions.push(SecurityAction {
                            rule,
                            kind: ActionKind::Encrypt { target },
                        });
                    }
                }
            }
        }
    }
    signs.into_iter().chain(encryptions).collect()
}

/// Verifiers and acceptors handle the security blocks protecting their targets. Targets
/// without the required operation are reported as missing; a target encrypted by a BCB
/// may have an encrypted BIB, so it is not missing a BIB.
fn receiver_actions<'a>(
    rules: impl Iterator<Item = &'a SecurityRule>,
    existing: &[SecurityBlock],
) -> Vec<SecurityAction<'a>> {
    let encrypted = targets_of(existing, SecurityOperation::Bcb);
    let mut handled = HashSet::new();
    let mut reported_missing = HashSet::new();
    let mut missing = Vec::new();
    let mut decryptions = Vec::new();
    let mut verifications = Vec::new();
    for rule in rules {
        let targets = target_numbers(rule);
        let protecting = existing.iter().filter(|block| {
            block.operation == rule.operation
                && block.targets.iter().any(|target| targets.contains(target))
        });
        for block in protecting {
            if !handled.insert(block.number) {
                continue;
            }
            match (rule.operation, rule.role) {
                (SecurityOperation::Bcb, _) => decryptions.push(SecurityAction {
                    rule,
                    kind: ActionKind::Decrypt { bcb: block.number },
                }),
                (SecurityOperation::Bib, SecurityRole::Acceptor) => {
                    verifications.push(SecurityAction {
                        rule,
                        kind: ActionKind::Accept { bib: block.number },
                    })
                }
                (SecurityOperation::Bib, _) => verifications.push(SecurityAction {
                    rule,
                    kind: ActionKind::Verify { bib: block.number },
                }),
            }
        }
        for target in targets {
            let protected = existing
                .iter()
                .any(|block| block.operation == rule.operation && block.targets.contains(&target))
                || (rule.operation == SecurityOperation::Bib && encrypted.contains(&target));
            if !protected && reported_missing.insert((rule.operation, target)) {
                missing.push(SecurityAction {
                    rule,
                    kind: ActionKind::Missing { target },
                });
            }
        }
    }
    missing
        .into_iter()
        .chain(decryptions)
        .chain(verifications)
        .collect()
}

fn targets_of(blocks: &[SecurityBlock], operation: SecurityOperation) -> HashSet<u64> {
    blocks
        .iter()
        .filter(|block| block.operation == operation)
        .flat_map(|block| block.targets.iter().copied())
        .collect()
}

/// Decodes the BIBs and BCBs of a bundle. Undecodable blocks are skipped, as are BIBs
/// encrypted by a BCB, which can only be decoded after decryption.
fn security_blocks(bundle: &Bundle) -> Vec<SecurityBlock> {
    let decoded: Vec<SecurityBlock> = bundle
        .canonicals
        .iter()
        .filter_map(|block| {
            let operation = match block.block_type {
                INTEGRITY_BLOCK => SecurityOperation::Bib,
                CONFIDENTIALITY_BLOCK => SecurityOperation::Bcb,
                _ => return None,
            };
            let CanonicalData::Unknown(data) = block.data() else {
                return None;
            };
            let asb = AbstractSecurityBlock::from_cbor(data).ok()?;
            Some(SecurityBlock {
                number: block.block_number,
                operation,
                targets: asb.security_targets,
            })
        })
        .collect();
    let encrypted = targets_of(&decoded, SecurityOperation::Bcb);
    decoded
        .into_iter()
        .filter(|block| {
            block.operation == SecurityOperation::Bcb || !encrypted.contains(&block.number)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::bpsec::config::{FailureAction, SecurityContext};
    use bp7::bpsec::{BcbSecurityContextParameter, BibSecurityContextParameter};
    use bp7::EndpointID;
    use std::convert::TryFrom;

    const HMAC_KEY: &[u8] = &[0x1a; 32];
    const AES_KEY: &[u8] = &[0x2b; 32];
    const IV: &[u8] = &[0x3c; 12];

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

    fn signed(mut bundle: Bundle, targets: &[u64]) -> (Bundle, u64) {
        let bib = bp7::bpsec::sign(
            &mut bundle,
            targets,
            HMAC_KEY,
            eid("dtn://node1/"),
            BibSecurityContextParameter::default(),
        )
        .unwrap();
        (bundle, bib)
    }

    /// Encrypts the target and, first, the BIBs protecting it (RFC 9172, 3.9).
    fn encrypted(mut bundle: Bundle, target: u64) -> (Bundle, u64) {
        let bibs: Vec<u64> = security_blocks(&bundle)
            .into_iter()
            .filter(|block| block.operation == Bib && block.targets.contains(&target))
            .map(|block| block.number)
            .collect();
        let mut bcb = 0;
        for block in bibs.into_iter().chain([target]) {
            bcb = bp7::bpsec::encrypt(
                &mut bundle,
                block,
                AES_KEY,
                IV,
                eid("dtn://node1/"),
                BcbSecurityContextParameter::default(),
            )
            .unwrap();
        }
        (bundle, bcb)
    }

    fn rule(
        name: &str,
        role: SecurityRole,
        operation: SecurityOperation,
        targets: &[SecurityTarget],
    ) -> SecurityRule {
        SecurityRule {
            name: name.into(),
            role,
            source: "**".into(),
            destination: "**".into(),
            operation,
            targets: targets.to_vec(),
            key: "key".into(),
            context: SecurityContext::default(),
            on_failure: FailureAction::Drop,
        }
    }

    use ProcessingStep::*;
    use SecurityOperation::*;
    use SecurityRole::*;
    use SecurityTarget::*;

    fn kinds(actions: &[SecurityAction]) -> Vec<ActionKind> {
        actions.iter().map(|action| action.kind.clone()).collect()
    }

    #[test]
    fn no_rules_no_actions() {
        for step in [Transmit, Receive, Forward] {
            assert!(evaluate(&[], step, &bundle()).is_empty());
        }
    }

    #[test]
    fn rules_only_apply_to_matching_bundles() {
        let mut sign = rule("sign", Source, Bib, &[Payload]);
        sign.destination = "dtn://*/sensors".into();
        assert_eq!(evaluate(&[sign.clone()], Transmit, &bundle()).len(), 1);

        sign.source = "dtn://node3/*".into();
        assert!(evaluate(&[sign], Transmit, &bundle()).is_empty());
    }

    #[test]
    fn roles_apply_at_their_processing_steps() {
        let rules = [
            rule("sign", Source, Bib, &[Payload]),
            rule("accept", Acceptor, Bib, &[Primary]),
        ];
        let sign = ActionKind::Sign {
            target: PAYLOAD_BLOCK_NUMBER,
        };
        let missing = ActionKind::Missing {
            target: PRIMARY_BLOCK_NUMBER,
        };
        assert_eq!(
            kinds(&evaluate(&rules, Transmit, &bundle())),
            vec![sign.clone()]
        );
        assert_eq!(kinds(&evaluate(&rules, Forward, &bundle())), vec![sign]);
        assert_eq!(kinds(&evaluate(&rules, Receive, &bundle())), vec![missing]);
    }

    #[test]
    fn sources_sign_before_encrypting_and_skip_protected_targets() {
        let rules = [
            rule("encrypt", Source, Bcb, &[Payload]),
            rule("sign", Source, Bib, &[Payload, Primary]),
            rule("sign-again", Source, Bib, &[Payload]),
        ];
        let actions = evaluate(&rules, Transmit, &bundle());
        assert_eq!(
            kinds(&actions),
            vec![
                ActionKind::Sign {
                    target: PAYLOAD_BLOCK_NUMBER
                },
                ActionKind::Sign {
                    target: PRIMARY_BLOCK_NUMBER
                },
                ActionKind::Encrypt {
                    target: PAYLOAD_BLOCK_NUMBER
                },
            ]
        );
        assert!(actions[..2].iter().all(|action| action.rule.name == "sign"));

        // a bundle signed and encrypted before needs nothing anymore
        let (bundle, _) = signed(bundle(), &[PAYLOAD_BLOCK_NUMBER]);
        let (bundle, _) = signed(bundle, &[PRIMARY_BLOCK_NUMBER]);
        let (bundle, _) = encrypted(bundle, PAYLOAD_BLOCK_NUMBER);
        assert!(evaluate(&rules, Forward, &bundle).is_empty());
    }

    #[test]
    fn encrypted_targets_are_not_signed() {
        let (bundle, _) = encrypted(bundle(), PAYLOAD_BLOCK_NUMBER);
        let rules = [rule("sign", Source, Bib, &[Payload, Primary])];
        assert_eq!(
            kinds(&evaluate(&rules, Forward, &bundle)),
            vec![ActionKind::Sign {
                target: PRIMARY_BLOCK_NUMBER
            }]
        );
    }

    #[test]
    fn verifiers_verify_and_acceptors_accept() {
        let (bundle, bib) = signed(bundle(), &[PAYLOAD_BLOCK_NUMBER]);
        let verify = [rule("verify", Verifier, Bib, &[Payload])];
        let accept = [rule("accept", Acceptor, Bib, &[Payload])];
        assert_eq!(
            kinds(&evaluate(&verify, Receive, &bundle)),
            vec![ActionKind::Verify { bib }]
        );
        assert_eq!(
            kinds(&evaluate(&accept, Receive, &bundle)),
            vec![ActionKind::Accept { bib }]
        );
    }

    #[test]
    fn first_matching_rule_handles_a_security_block() {
        let (bundle, bib) = signed(bundle(), &[PAYLOAD_BLOCK_NUMBER]);
        let rules = [
            rule("verify", Verifier, Bib, &[Payload]),
            rule("accept", Acceptor, Bib, &[Payload]),
        ];
        let actions = evaluate(&rules, Receive, &bundle);
        assert_eq!(kinds(&actions), vec![ActionKind::Verify { bib }]);
        assert_eq!(actions[0].rule.name, "verify");
    }

    #[test]
    fn unprotected_targets_are_missing() {
        let (bundle, bib) = signed(bundle(), &[PAYLOAD_BLOCK_NUMBER]);
        let rules = [rule("accept", Acceptor, Bib, &[Payload, Primary])];
        assert_eq!(
            kinds(&evaluate(&rules, Receive, &bundle)),
            vec![
                ActionKind::Missing {
                    target: PRIMARY_BLOCK_NUMBER
                },
                ActionKind::Accept { bib },
            ]
        );
    }

    #[test]
    fn acceptors_decrypt_before_verifying() {
        let (bundle, bib) = signed(bundle(), &[PRIMARY_BLOCK_NUMBER]);
        let (bundle, bcb) = encrypted(bundle, PAYLOAD_BLOCK_NUMBER);
        let rules = [
            rule("accept-bib", Acceptor, Bib, &[Primary]),
            rule("decrypt", Acceptor, Bcb, &[Payload]),
        ];
        assert_eq!(
            kinds(&evaluate(&rules, Receive, &bundle)),
            vec![ActionKind::Decrypt { bcb }, ActionKind::Accept { bib }]
        );
    }

    #[test]
    fn encrypted_targets_are_not_missing_their_bib() {
        // the BIB of the payload is encrypted along with it
        let (bundle, _) = signed(bundle(), &[PAYLOAD_BLOCK_NUMBER]);
        let (bundle, bcb) = encrypted(bundle, PAYLOAD_BLOCK_NUMBER);
        let rules = [
            rule("accept-bib", Acceptor, Bib, &[Payload]),
            rule("decrypt", Acceptor, Bcb, &[Payload]),
        ];
        assert_eq!(
            kinds(&evaluate(&rules, Receive, &bundle)),
            vec![ActionKind::Decrypt { bcb }]
        );
    }
}
