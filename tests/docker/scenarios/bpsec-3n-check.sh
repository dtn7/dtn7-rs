#!/bin/bash
# Checks the BPSec scenario: start it first with `../run.sh bpsec-3n.yml`.
#
# n1 sends a bundle to n3, which n1 signs and encrypts, n2 verifies and n3 decrypts and
# accepts. A second, unsigned bundle from n1 is dropped by n2. Requires python3.

set -e -o pipefail

DOCKERCMD=docker
# use podman if available
if command -v podman &> /dev/null; then
  DOCKERCMD=podman
fi

N1=bpsec-3n-n1-1
N2=bpsec-3n-n2-1
N3=bpsec-3n-n3-1
SECRET="sensor data, for n3 only"

fail () {
  echo "FAILED: $1"
  exit 1
}

# waits up to 60 seconds for a command to succeed
wait_for () {
  for _ in $(seq 60); do
    if "$@" > /dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

# prints a counter of a rule from /status/bpsec of a node, e.g., `counter $N1 sign signed`
counter () {
  $DOCKERCMD exec "$1" curl -s http://127.0.0.1:3000/status/bpsec | python3 -c "
import json, sys
rules = {rule['name']: rule for rule in json.load(sys.stdin)['rules']}
print(rules['$2']['statistics']['$3'])"
}

expect_counter () {
  local value
  value=$(counter "$1" "$2" "$3")
  echo "$1 $2 $3: $value"
  [ "$value" -ge "$4" ] || fail "expected $2 $3 >= $4 on $1"
}

knows_peer () {
  $DOCKERCMD exec "$1" curl -s http://127.0.0.1:3000/status/peers | grep -q "\"$2\""
}

echo "Waiting for peer discovery..."
wait_for knows_peer $N1 n2 || fail "n1 did not discover n2"
wait_for knows_peer $N2 n3 || fail "n2 did not discover n3"

$DOCKERCMD exec $N3 dtnrecv -r incoming
$DOCKERCMD exec $N1 dtnrecv -r unsigned

echo "Sending a protected and an unsigned bundle from n1 to n3..."
BID=$(echo -n "$SECRET" | $DOCKERCMD exec -i $N1 dtnsend -r dtn://n3/incoming | sed -n 's/^Bundle-Id: //p')
echo -n "unsigned" | $DOCKERCMD exec -i $N1 dtnsend -s dtn://n1/unsigned -r dtn://n3/incoming

# the bundle as stored and sent by n1
$DOCKERCMD exec $N1 curl -s -f -o /tmp/protected "http://127.0.0.1:3000/download?$BID" \
  || fail "bundle $BID not stored on n1"
if $DOCKERCMD exec $N1 grep -q "$SECRET" /tmp/protected; then
  fail "n1 sent the payload unencrypted"
fi
echo "n1 sent the payload encrypted"

received () {
  $DOCKERCMD exec $N3 dtnrecv -e incoming | grep -q "$SECRET"
}
wait_for received || fail "n3 did not receive the protected bundle"
echo "n3 received the protected bundle"

# give the unsigned bundle time to arrive, if n2 forwarded it
sleep 5
if $DOCKERCMD exec $N3 dtnrecv -e incoming | grep -q "unsigned"; then
  fail "n3 received the unsigned bundle"
fi

expect_counter $N1 sign signed 2
expect_counter $N1 encrypt encrypted 2
expect_counter $N2 verify verified 1
expect_counter $N2 verify missing 1
expect_counter $N2 verify dropped 1
expect_counter $N3 decrypt decrypted 2
expect_counter $N3 accept accepted 2

echo "PASSED"
