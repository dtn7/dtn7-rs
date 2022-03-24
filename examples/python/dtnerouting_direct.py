import websocket
import json
import sys

# Minimal example for external routing with a direct routing strategy.
#
# You can specify an address to connect to as first argument:
# > dtnerouting_direct.py 127.0.0.1:3002
#
# Requirements:
# pip3 install websocket-client

# Dict of available peers
peers = {}

# Dict of delivered bundles
delivered = {}


# Extract node name from [1, "//nodex/..."]
def get_node_from_endpoint(endpoint):
    url = endpoint[1][2:]
    return url[:url.index("/")]


#
# Base WebSocket Handler
#


def on_open(ws):
    print("Connected")


def on_message(ws, raw):
    print(raw)

    msg = json.loads(raw)

    switcher = {
        "RequestSenderForBundle": on_sender_for_bundle,
        "PeerState": on_peer_state,
        "PeerEncountered": on_peer_encountered,
        "PeerDropped": on_peer_dropped,
        "SendingFailed": on_sending_failed,
        "Timeout": on_sending_timeout,
    }

    switcher.get(msg["type"])(msg)


#
# Packet Handlers
#


def on_sender_for_bundle(msg):
    dest = msg["bp"]["destination"]
    bundle_id = msg["bp"]["id"]

    print("===> SenderForBundle: To", dest[1], bundle_id)

    # Check if already delivered
    global delivered
    if bundle_id in delivered:
        print("Bundle was already delivered!")
        return

    # Check if peer is known
    target = peers.get(get_node_from_endpoint(dest))
    if target is not None:
        req_cla_list = msg['clas']
        peer_cla_list = target["cla_list"]
        target_clas = []

        print(target.eid)

        # Search for possible clas
        for c in peer_cla_list:
            if req_cla_list.index(c[0]) >= 0:
                target_clas.append({
                    "remote": target["addr"],
                    "agent": c[0],
                    "port": c[1],
                    "next_hop": target["eid"]
                })

        # Build and send response
        resp = {
            "type": "SenderForBundleResponse",
            "bp": msg["bp"],
            "clas": target_clas,
            "delete_afterwards": True
        }

        # If some cla was found set as delivered
        if len(target_clas) > 0:
            delivered[bundle_id] = True
            print("Peer is known and sending is requested via", target_clas)
        else:
            print("Peer is known but no cla could be selected")

        # Send response
        wsapp.send(json.dumps(resp))
    else:
        print("Peer not directly known")


def on_peer_state(msg):
    print("===> PeerState")

    global peers
    peers = msg["peers"]


def on_peer_encountered(msg):
    print("===> PeerEncountered: Encountered", msg["name"])

    global peers
    peers[msg["name"]] = msg


def on_peer_dropped(msg):
    print("===> PeerDropped: Dropping", msg["name"])

    global peers
    del peers[msg["name"]]


def on_sending_failed(msg):
    print("===> SendingFailed: For Bundle", msg['bid'])

    # Remove from delivered if sending failed
    global delivered
    del delivered[msg['bid']]


def on_sending_timeout(msg):
    print("===> Timeout: For Bundle", msg["bp"]["id"])

    # Remove from delivered if sending was not received fast enough
    global delivered
    del delivered[msg["bp"]["id"]]


#
# Start WebSocket Client
#


addr = "ws://127.0.0.1:3000/ws/erouting" if len(sys.argv) < 2 else "ws://" + sys.argv[1] + "/ws/erouting"
print("Connecting to", addr)

wsapp = websocket.WebSocketApp(addr, on_message=on_message, on_open=on_open)
if wsapp.run_forever():
    print("Closed!")