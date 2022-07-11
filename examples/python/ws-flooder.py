#!/usr/bin/env python3

# A simple flooding script to benchmark the WS client interface
#
# Requirements:
# pip3 install websocket-client
# pip3 install cbor2

import sys
import urllib.request
import websocket
from cbor2 import dumps, loads

# how many bundles to send
flood_len = 1000

# transfer all bundles in bulk and wait for ack afterwards or send bundles one by one and wait for acks
bulk_transfer = True

# Ready to receive data?
recv_data = False

# Get the local node ID
local_node = urllib.request.urlopen(
    "http://127.0.0.1:3000/status/nodeid").read().decode("utf-8")
print("Running dummy service on " + local_node)

# Define service endpoint, "echo" for 'dtn' nodes and '7' for 'ipn' nodes
service = "dummy"
if local_node.startswith('ipn'):
    service = 99

# Prior to receiving anything register the local service endpoint
register = urllib.request.urlopen(
    "http://127.0.0.1:3000/register?"+service).read()
print(register)

counter = 0


def on_open(ws):
    print("Connected")

    # first switch to 'data' mode
    # we can then receive decoded bundles, giving us direct access to the payload
    # default would be 'bundle' mode where we have to manually decode the complete bundle
    ws.send("/data")


def send_pkt(ws):
    global counter
    out = {}
    out["src"] = local_node
    out["dst"] = "dtn://echo.dtn/dummy"
    out["delivery_notification"] = False
    out["lifetime"] = 3600*1000
    out["data"] = "Hello World".encode()
    #[print(key,':',value) for key, value in out.items()]

    # encode the response as a CBOR byte string
    # print(out)
    out_cbor = dumps(out)
    #print("encoded: ", out_cbor)
    #hexstr = "".join(format(i, "02x") for i in out_cbor)
    #print("hexstr: " + hexstr)

    # send cbor encoded data as binary (opcode = 2)
    #print("sending packet")
    ws.send(out_cbor, opcode=2)


def on_message(ws, message):
    global recv_data
    global service
    global counter
    global flood_len
    global bulk_transfer

    #print(recv_data, message)
    if not recv_data:
        if message == "200 tx mode: data":
            print("mode changed to `data`")
            # after the mode was set we subscribe to the echo service previously registered
            ws.send("/subscribe " + service)
        elif message == "200 subscribed":
            print("succesfully subscribed")
            # after subscribing we are ready to receive bundles
            recv_data = True
            print("Sending packets")
            if bulk_transfer:
                for i in range(flood_len):
                    send_pkt(ws)
            else:
                send_pkt(ws)
        else:
            print("received: " + message)

    else:
        if message[0:3] == '200':
            # text messages starting with '200' inidicate successful transmission
            print(".", end='')
            # print(counter)
            sys.stdout.flush()
            counter += 1
            if counter < flood_len:
                if not bulk_transfer:
                    send_pkt(ws)
            else:
                print("\nDone")
                ws.close()
                sys.exit(0)
        else:
            print("Received something odd: ", message)


# Enable debug output from websocket engine
# websocket.enableTrace(True)

# Connect to default port of dtn7 running on the local machine
wsapp = websocket.WebSocketApp(
    "ws://127.0.0.1:3000/ws", on_message=on_message, on_open=on_open)
wsapp.run_forever()
