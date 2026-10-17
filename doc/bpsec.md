# Bundle Protocol Security (BPSec)

dtnd can protect bundles with [BPSec (RFC 9172)](https://www.rfc-editor.org/rfc/rfc9172.html)
and its default security contexts [(RFC 9173)](https://www.rfc-editor.org/rfc/rfc9173.html):

* **Integrity** with block integrity blocks (BIBs), using BIB-HMAC-SHA2
* **Confidentiality** with block confidentiality blocks (BCBs), using BCB-AES-GCM, optionally
  with wrapped keys (AES key wrap)

Security rules in the configuration file decide which bundles are signed, verified,
encrypted or decrypted at a node. BPSec is part of the default build (cargo feature `bpsec`),
but inactive as long as no rules are configured.

## Roles

Each rule gives the node a role for the bundles it matches (RFC 9172, 2.2):

| Role | BIB | BCB | When |
|---|---|---|---|
| `source` | signs the targets | encrypts the targets | a local application sends a bundle, or the node forwards one (e.g., as gateway) |
| `verifier` | checks the BIB and keeps it | *not possible* | a bundle is received |
| `acceptor` | checks the BIB and removes it | decrypts the targets and removes the BCB | a bundle is received |

Acceptors remove the security blocks, so the bundle is delivered or forwarded without them.
Usually the destination node is the acceptor, while verifiers check bundles along the path,
e.g., at the entry of a network.

## Configuration

```toml
[bpsec]
# one <key id>.key file per key, containing the hex encoded key
keys = "/etc/dtn7/keys"

[[bpsec.rule]]
name = "sign-sensors"
role = "source"
source = "dtn://node1/"
destination = "dtn://*/sensors"
operation = "bib"
targets = ["primary", "payload"]
key = "hmac-sensors"
context = { sha-variant = "sha384" }

[[bpsec.rule]]
name = "encrypt-sensors"
role = "source"
source = "dtn://node1/"
destination = "dtn://*/sensors"
operation = "bcb"
targets = ["payload"]
key = "kek-sensors"
context = { aes-variant = "aes256", wrapped-key = true }
```

| Setting | Values | Default |
|---|---|---|
| `name` | unique name, shown in logs and `/status/bpsec` | required |
| `role` | `source`, `verifier`, `acceptor` | required |
| `source`, `destination` | glob patterns for the bundle source and destination | `"**"` |
| `operation` | `bib`, `bcb` | required |
| `targets` | `primary`, `payload` (BCBs only `payload`) | required |
| `key` | key id, i.e., the key file `<key id>.key` | required |
| `context` | BIB: `sha-variant` = `sha256`, `sha384`, `sha512`<br>BCB: `aes-variant` = `aes128`, `aes256`, `wrapped-key` = `true`, `false` | `sha384`<br>`aes256`, no wrapped key |
| `on-failure` | `drop`, `remove-operation`, `log` | `drop` |

In the globs, `*` matches within a path segment of an endpoint ID and `**` across
segments: `dtn://node1/*` matches `dtn://node1/sensor`, but not `dtn://node1/` or
`dtn://node1/sensors/temperature`, while `dtn://node1/**` matches all of them.

Rules are checked at startup; dtnd refuses to start with invalid rules, e.g., unknown or
misspelled settings, missing keys, keys of the wrong length, or combinations RFC 9172
forbids, such as encrypting the primary block.

### Rule Matching

* A security block is handled by the first matching rule, so specific rules go first.
* Sources protect each target once: targets already signed or encrypted, by an earlier rule
  or another node, are skipped. Each target gets its own BIB and BCB.
* Bundles without matching rules are processed as usual, also if they carry security
  blocks: a node without the keys can not check them.
* The targets of encrypted BIBs are unknown until decrypted, so BCBs encrypting BIBs are
  decrypted by the first matching BCB rule. Use one BCB rule per bundle source.

### Failures

If a security operation fails, e.g., a BIB does not match its targets or a decryption
fails, or a rule demands a security operation the bundle lacks, `on-failure` decides:

* `drop` deletes the bundle. A status report with the reason (missing, failed or unknown
  security operation) is sent only if the bundle requests deletion reports and status
  reports are enabled (`generate-status-reports`).
* `remove-operation` removes the failed BIB and keeps the bundle, only for BIB verifiers and
  acceptors. BCBs can not be removed without decrypting them.
* `log` only logs the failure. Sources can only `drop` or `log`.

## Keys

Each key is a file `<key id>.key` in the key directory, containing the hex encoded key:

```
$ mkdir -p /etc/dtn7/keys && cd /etc/dtn7/keys
$ openssl rand -hex 32 > hmac-sensors.key
$ openssl rand -hex 32 > kek-sensors.key
$ chmod 600 *.key
```

* BIB keys can have any length, 32 bytes or more are recommended.
* BCB keys have 16 bytes for `aes128` and 32 bytes for `aes256`.
* With `wrapped-key = true`, the key is a key encryption key of 16, 24 or 32 bytes. Each BCB
  is encrypted with a new random key, which is wrapped with the key encryption key and
  sent along in the BCB.

dtnd warns about key files other users can read. Keys never appear in logs or HTTP
responses. Distributing the keys to the nodes is up to the operator.

Keys can be replaced at runtime, e.g., for key rotation:

```
$ curl -X POST http://127.0.0.1:3000/bpsec/keys/reload
Reloaded 2 keys
```

A reload that fails, e.g., due to an invalid key file or a key missing for a rule, keeps
the current keys.

## Status

`/status/bpsec` shows the rules with their statistics and the number of loaded keys:

```
$ curl http://127.0.0.1:3000/status/bpsec
{
  "active": true,
  "keys": 2,
  "rules": [
    {
      "name": "sign-sensors",
      "role": "source",
      ...
      "statistics": {
        "signed": 2,
        "verified": 0,
        "accepted": 0,
        "encrypted": 0,
        "decrypted": 0,
        "missing": 0,
        "failed": 0,
        "dropped": 0
      }
    }
  ]
}
```

`dropped` counts bundles, all other counters count security operations, e.g., `encrypted`
the BCBs added. Both endpoints are only reachable from localhost, as they reveal the
security policy of the node.

## Limitations

* Only the primary and payload block can be targets. Hop count, previous node and bundle
  age blocks change on every hop.
* A BIB covers the primary block including its CRC. Signing the primary block removes its
  CRC, accepting its BIB restores it, so a node signing or accepting the primary block of
  a bundle with BIBs of other nodes invalidates these BIBs.
* Matching on peers or convergence layers is not supported.

## Example

The Docker scenario [`tests/docker/scenarios/bpsec-3n.yml`](../tests/docker/scenarios/bpsec-3n.yml)
runs three nodes: `n1` signs and encrypts bundles to `n3`, `n2` verifies them on the way and
drops unsigned ones, `n3` decrypts and accepts them.

```
$ cd tests/docker/scenarios
$ ../run.sh bpsec-3n.yml
$ ./bpsec-3n-check.sh    # in another terminal
```
