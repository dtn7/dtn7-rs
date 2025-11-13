#!/usr/bin/env python3

# More or less minimal example of a dtn7 echo service using websockets
#
# Requirements:
# pip3 install websocket-client
# pip3 install cbor2

import urllib.request
import websocket
from cbor2 import dumps, loads
import sys

PORT = sys.argv[1] if len(sys.argv) > 1 else 3000

# Ready to receive data?
recv_data = False

# Get the local node ID
local_node = urllib.request.urlopen(f"http://127.0.0.1:{PORT}/status/nodeid").read().decode("utf-8")
print("Running echo service on " + local_node)

# Define service endpoint, "echo" for 'dtn' nodes and '7' for 'ipn' nodes
service = "echo"
if local_node.startswith('ipn'):
    service = 7

# Prior to receiving anything register the local service endpoint
register = urllib.request.urlopen(f"http://127.0.0.1:{PORT}/register?{service}").read()
print(register)

def on_open(ws):
    print("Connected")

    # first switch to 'data' mode
    # we can then receive decoded bundles, giving us direct access to the payload
    # default would be 'bundle' mode where we have to manually decode the complete bundle
    ws.send("/data")

def on_message(ws, message):
    global recv_data
    global service

    #print(recv_data, message)
    if not recv_data:
        if message == "200 tx mode: data":
            print("mode changed to `data`")
            # after the mode was set we subscribe to the echo service previously registered
            if local_node.startswith('ipn'):
                service = "{}.{}".format(local_node.split(".")[0], service)
            ws.send("/subscribe " + service)
        elif message == "200 subscribed":
            print("successfully subscribed")
            # after subscribing we are ready to receive bundles
            recv_data = True
    else:
        if message[0:3] == '200':
            # text messages starting with '200' indicate successful transmission
            print("sent reply")
        else:
            #hexstr = "".join(format(i, "02x") for i in message)
            #print("decoding: " + hexstr)

            # load CBOR message as dictionary
            data = loads(message)

            print("received new bundle: " + data['bid'])

            # construct the echo reply, swapping 'src' and 'dst'
            out = {
                'src': data['dst'],
                'dst': data['src'],
                'delivery_notification' : False,
                'lifetime' : 3600*24*1000,
                'data' : data['data']
            }
            #[print(key,':',value) for key, value in out.items()]

            # encode the response as a CBOR byte string
            out_cbor = dumps(out)

            #hexstr = "".join(format(i, "02x") for i in out_cbor)
            #print("response: " + hexstr)

            # send cbor encoded data as binary (opcode = 2)
            ws.send(out_cbor, opcode=2)

# Enable debug output from websocket engine
# websocket.enableTrace(True)

# Connect to default port of dtn7 running on the local machine
wsapp = websocket.WebSocketApp(f"ws://127.0.0.1:{PORT}/ws", on_message=on_message, on_open=on_open)
wsapp.run_forever()

