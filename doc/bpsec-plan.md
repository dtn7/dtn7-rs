# BPSec Policies in dtn7-rs – Implementation Plan

Status: **implemented**, phases 0 to 6 done, the key directory watcher is open (local working document)

## Goal and Scope

dtn7-rs gets configurable BPSec (RFC 9172) security policies: which bundles are signed,
verified, encrypted or decrypted at this node, with which keys, and what happens on
failures.

The work is split between the two projects as follows:

- **bp7-rs** provides the security mechanisms (encoding, cryptography, RFC 9172/9173 rules).
- **dtn7-rs** provides the policy: rule model, configuration, key management and the
  enforcement within the bundle processing pipeline.

Policy types are not part of the bp7 public API, so changes to the policy do not require
coordinated releases of bp7, dtn7-plus and d7sneakers.

## Decisions

| Topic | Decision |
|---|---|
| Feature flag | The cargo feature `bpsec` is **enabled by default**, but BPSec is **inactive without configured rules**. |
| Rule matching | **Source and destination globs** are sufficient for the first iteration (same glob matching as the static routing agent). |
| Key updates | Keys must be **updatable at runtime**, see [Key Management](#key-management). |
| Status reports | Status reports for security failures are **only sent when requested** by the bundle's status report flags. |
| bp7 release | bp7 with the new BPSec support will be released soon as a breaking release, after a few more features land. Development starts against the bp7 git version and switches to the release before merging. |

## What bp7-rs Provides

The unreleased bp7-rs (`master`, after v0.11.0) offers bundle-level security operations
with the `bpsec` feature:

| Function | Purpose |
|---|---|
| `bib::sign(bundle, targets, key, source, params)` | Add a BIB-HMAC-SHA2 BIB for the given targets |
| `bib::verify(bundle, bib, key)` | Verify a BIB without changing the bundle (verifier role) |
| `bib::accept(bundle, bib, key, crc_type)` | Verify, remove the BIB and restore target CRCs (acceptor role) |
| `bcb::encrypt(bundle, target, key, iv, source, params)` | Encrypt a block with BCB-AES-GCM |
| `bcb::decrypt(bundle, bcb, key, crc_type)` | Decrypt and remove a BCB (acceptor role) |
| `wrap_key` / `unwrap_key` | AES key wrap for per-bundle content keys |
| `Bundle::decode_bibs()` / `decode_bcbs()` | Inspect the security blocks of a bundle |
| `BpsecError::status_report_reason()` | RFC 9172 status report reason for a failure |

bp7 also enforces the RFC 9172 constraints on BIB/BCB interactions and security targets.

## Phases

### Phase 0 – Prerequisites

1. Switch to the new bp7 release (git dependency during development) and release
   d7sneakers and dtn7-plus against it, as bp7 types are part of their public APIs.
2. Add the dtn7 cargo feature `bpsec` (default on), enabling the `bpsec` feature of bp7.
3. **Fix the block type check in `receive()`**: all block types `>= 11` are currently
   treated as unknown, including BIB (11) and BCB (12). Their "delete bundle" and
   "remove block" flags are thus applied incorrectly. With the `bpsec` feature, BIB and
   BCB are known block types; without it, they stay unknown as RFC 9172 demands for
   nodes not implementing BPSec.

### Phase 1 – Configuration and Keys (no enforcement)

New module `core/bpsec/` (only built with the `bpsec` feature): `config.rs`, `keys.rs`,
`policy.rs`, `enforcement.rs`.

Configuration in the dtnd TOML file, parsed in `dtnconfig.rs`:

```toml
[bpsec]
keys = "/etc/dtn7/keys"            # one <key id>.key file per key, hex encoded

[[bpsec.rule]]
name = "sign-outgoing"
role = "source"                    # source | verifier | acceptor
source = "dtn://node1/*"           # glob, "**" if not given
destination = "dtn://*/sensors"    # glob, "**" if not given
operation = "bib"                  # bib | bcb
targets = ["payload", "primary"]
key = "hmac-sensors"
context = { sha-variant = "sha384" }
on-failure = "drop"                # drop | remove-operation | log
```

Keys use kebab case like the rest of the dtnd configuration; unknown keys are
rejected to catch typos. In globs, `*` matches within a path segment of an endpoint ID
and `**` across segments, so the default matching all bundles is `**`.

Without any `[[bpsec.rule]]`, BPSec processing is inactive and bundles are processed as
before.

Rules are validated at startup: unknown key ids, invalid targets and combinations
forbidden by RFC 9172 (e.g., a BIB targeting a security block) are rejected with a clear
error.

#### Key Management

- `KeySet` loads one `<key id>.key` file per key (hex encoded) from the configured
  directory, warns on too permissive file permissions, and zeroizes key material when
  dropped (`zeroize`). Keys never appear in logs, debug output or HTTP responses.
  Concrete types instead of a `KeyStore` trait, as there is only one key source so far.
- The active key set is shared behind a lock and **replaced atomically** on reload, so
  security operations always see either the old or the new key set, never a mix.
- **Runtime updates**, in this order:
  1. A REST command `POST /bpsec/keys/reload` (localhost only, like the other control
     endpoints) reloads the key directory. A reload that fails (e.g., unreadable or
     invalid key file) keeps the current keys and returns an error.
  2. Optionally, a directory watcher (`notify` crate, behind a config option) triggers
     the same reload when files in the key directory change, debounced to handle
     editors writing files in several steps.

  The REST command comes first as it is explicit, deterministic and easy to test; the
  watcher reuses the same reload function.
- Rules referencing a key that is missing after a reload fail like any other security
  operation, according to their `on_failure` action.

### Phase 2 – Policy Engine (pure functions)

Implemented in `core/bpsec/policy.rs`:

```rust
fn evaluate<'a>(rules: &'a [SecurityRule], step: ProcessingStep,
                bundle: &Bundle) -> Vec<SecurityAction<'a>>
```

- `ProcessingStep`: `Transmit` (bundle of a local application), `Receive`, `Forward`.
  Source rules apply at `Transmit` and `Forward`, verifier and acceptor rules at
  `Receive`. A separate `Deliver` step is not needed: acceptors handle bundles at
  reception, before they are delivered locally.
- `SecurityAction`: the demanding rule (key, context, failure action) and the kind:
  `Sign`, `Encrypt`, `Verify`, `Accept`, `Decrypt` or `Missing` (a rule demands an
  operation the bundle lacks, handled as failure). Targets and security blocks are
  identified by block number.
- No globals or I/O, fully unit testable, and movable into a separate crate later
  without changes.
- A security block is handled by the first matching rule, like firewall rules.
- Security sources protect each target once and **sign each target with its own BIB**:
  a BIB has to be encrypted along with its targets (RFC 9172, 3.9), so a BIB for several
  targets would hide the protection of all of them once one is encrypted.
- Ordering at security sources: BIBs before BCBs; encrypted targets are not signed.
  `Encrypt` covers the target and the BIBs protecting it.
- Ordering at receivers: missing operations, decryption, then verification and
  acceptance. BIBs encrypted by a BCB are only visible after decryption, so the
  enforcement evaluates the policy again after decrypting.
- BCBs can not be checked without decrypting them, so verifier rules for BCBs are
  rejected by the configuration validation.

### Phase 3 – Enforcement in the Processing Pipeline

| Step | Location | Roles |
|---|---|---|
| Local application sends | `send_bundle()`, after `validate_bundle`, before storing | source |
| Reception | `receive()`, after the unknown block handling, before storing | verifier, acceptor |
| Forwarding | `forward()`, after hop count, previous node and bundle age updates | source (e.g., gateways) |

No hook in `local_delivery()`: acceptors handle bundles at reception (see Phase 2).

Implemented in `core/bpsec/enforcement.rs`, called through `bpsec::enforce(step, bundle)`,
which returns `Verdict::Process` or `Verdict::Drop(reason)` and returns immediately
without rules:

- Failure actions are part of this phase: `drop` deletes the bundle via `delete()`
  (status report only if requested), `remove-operation` removes the failed BIB, `log`
  continues. `remove-target` was dropped: the only targets, primary and payload block,
  can not be removed.
- BCB rules are rejected by the configuration until Phase 5.
- Signing the primary block removes its CRC, accepting its BIB restores it. Both change
  the primary block covered by the other BIBs (integrity scope flags), so the primary
  block is signed first and its BIB accepted last.
- bp7 only sets the CRC types of changed blocks; the enforcement calculates the CRCs.

- Operations changing the bundle (accept, decrypt, sign on local transmission) are
  written back to the store. Signing in `forward()` only changes the copy that is sent,
  so retries start from the unchanged stored bundle.
- First iteration: BIB targets are limited to the payload and primary block. Hop count,
  previous node and bundle age blocks change on every hop and would invalidate
  signatures.
- Bundles with security blocks but without a matching rule are forwarded unchanged and
  never dropped; a node without the keys can not verify them.

### Phase 4 – Failure Handling and Visibility

- Done in Phase 3: failure actions, and status reports via `delete()` only if requested,
  with the reason "missing", "failed" or "unknown security operation".
- Counters per rule (signed, verified, accepted, missing, failed, dropped), kept in the
  bpsec module instead of STATS so the `/status/info` format stays unchanged. They are
  counted per bundle and merged afterwards, so no global lock is held during the
  security operations, and reset by `init()`.
- `/status/bpsec` shows whether BPSec is active, the number of loaded keys and the
  rules with their counters, without key material. It is only reachable from
  localhost, as it reveals the security policy of the node.

### Phase 5 – BCB

- Encryption at security sources and decryption at acceptors with the same rule
  machinery.
- Each block gets its own BCB with a new IV from the OS random number generator
  (`rand::rngs::OsRng`), as a BCB with several targets shares the IV between them. A BIB
  of a target is encrypted first, by its own BCB; the bundle is only changed if all
  encryptions succeed. A test checks that IVs are never reused.
- Wrapped keys (AES-KW): `context = { wrapped-key = true }` makes the key of a BCB rule a
  key encryption key (16, 24 or 32 bytes), wrapping a random content encryption key per
  BCB. Decryption unwraps automatically, as the wrapped key is part of the BCB.
- At reception, the BCB actions come first; BIBs encrypted by a BCB only become visible
  after decrypting, so the policy is evaluated again for the BIB actions. The targets of
  encrypted BIBs are unknown until decrypted, so BCBs encrypting BIBs are decrypted by
  the first matching BCB rule.
- BCB verifiers are rejected (a BCB can not be checked without decrypting it), as is
  `remove-operation` for BCB rules (bp7 can not remove a BCB without decrypting it).
- Statistics count encrypted and decrypted blocks.

### Phase 6 – Tests and Documentation

- Unit tests for configuration, keys, policy, enforcement and statistics; an integration
  test through `receive()` in `tests/processing-tests.rs`.
- Docker scenario `tests/docker/scenarios/bpsec-3n.yml`, built from the local sources:
  n1 signs and encrypts bundles to n3, n2 verifies them and drops unsigned bundles, n3
  decrypts and accepts them. `bpsec-3n-check.sh` sends a protected and an unsigned
  bundle and checks delivery, that n1 sends the payload encrypted, and the statistics of
  all nodes. The test keys are part of the repository and marked as such.
- The Dockerfiles needed newer Rust (bp7 0.12 uses edition 2024): `rust:1.90-bookworm`,
  matching the bookworm runtime, and `rust:1.90-alpine`; dtn7-plus is installed in its
  released version 0.9.0 instead of an old git revision.
- User guide `doc/bpsec.md`, the endpoints in `doc/http-client-api.md`, README and
  example configuration.
- The CHANGELOG is generated from the conventional commits by `git cliff` in `release.sh`.
- The statistics count security blocks: `encrypted` counts the BCBs of a target and its
  BIBs, matching `decrypted` at the acceptor.

## Order of Work

Each step consists of one or a few conventional commits; from step 4 on, every step
leaves a usable state.

1. Phase 0: bp7 git dependency, `bpsec` feature, block type fix
2. Phase 1: configuration and key store, REST key reload
3. Phase 2: policy engine
4. BIB verifier and acceptor on reception and delivery
5. BIB source on local transmission and forwarding
6. Failure actions, status reports, statistics and `/status/bpsec`
7. Phase 5: BCB
8. Optional key directory watcher
9. Docker scenario and documentation
10. Switch to the released bp7 version (and released dtn7-plus/d7sneakers) before merging

## Open Points

- A security source signing the primary block of a bundle that already carries BIBs of
  other targets (e.g., a gateway) removes the primary CRC and thereby invalidates these
  BIBs; likewise, accepting the primary BIB invalidates remaining BIBs. Possible fix in
  bp7: keep the primary CRC as is, or exclude it from the IPPT.

- Key distribution itself stays out of scope; keys are placed in the key directory by
  other means.
- Matching on peers or CLAs (e.g., "sign everything leaving via the satellite link") is
  deferred until after the first iteration.
- Exact bp7 release version and timing, see the ongoing bp7 work.
