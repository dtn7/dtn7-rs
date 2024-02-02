# dtn7-rs HTTP client API documentation

This document contains a brief description of the various HTTP API endpoints of *dtn7-rs*.
Additionally, a short description of the websocket interface for external application agents is given.

The default port for the dtn7 HTTP interface is port *3000*.

Depending on the request, all API calls return a plaintext string, JSON or binary data.

## General UI 

Under `/` a general system overview can be found. 
This is meant to be viewed with a standard web browser such as firefox, chrome, lynx or w3m.

## Localhost-only API

These API endpoints can only be called from localhost for security reasons.

### **POST** `/send?dst=<EID>&lifetime=<LIFETIME>`

Construct a new bundle with the given parameters. 
The bundle payload is sent as the body of the *POST* request.
The URL parameters `dst` and `lifetime` are used to set the corresponding bundle fields.

```
$ curl -X POST -d 'hello world' "http://127.0.0.1:3000/send?dst=dtn://node3/incoming&lifetime=5m"
Sent payload with 11 bytes
```

### **GET** `/register?<ENDPOINT>`

Register a new application endpoint. 
This can be either a local singleton endpoint, e.g., `mailbox`, or a group endpoint such as `dtn://global/~news`. 

```
$ curl http://127.0.0.1:3000/register?mailbox
Registered dtn://node1/mailbox

$ curl http://127.0.0.1:3000/status/eids
[
  "dtn://node1",
  "dtn://node1/incoming",
  "dtn://node1/mailbox"
]

$ curl http://127.0.0.1:3000/register?dtn://global/~news
Registered URI: dtn://global/~news

$ curl http://127.0.0.1:3000/status/eids
[
  "dtn://node1",
  "dtn://node1/incoming",
  "dtn://node1/mailbox",
  "dtn://global/~news"
]
```

### **GET** `/unregister?<ENDPOINT>`

Unregister a previously registered application agent endpoint.

```
$ curl http://127.0.0.1:3000/unregister?mailbox
Unregistered dtn://node1/mailbox

$ curl http://127.0.0.1:3000/status/eids
[
  "dtn://node1",
  "dtn://node1/incoming",
  "dtn://global/~news"
]
```

### **GET** `/peers/add?p=<PEER_CONNECT_URL>&p_t=<STATIC|DYNAMIC>`

Adds a new peer connection or updates an existing one, setting the time of last contact to now.

```
$ curl "http://127.0.0.1:3000/peers/add?p=tcp://127.0.0.1:4223/node2&p_t=STATIC"
Added new peer
```
### **GET** `/peers/del?p=<PEER_CONNECT_URL>`

Deletes a peer from die internal peer list.

```
$ curl "http://127.0.0.1:3000/peers/del?p=tcp://127.0.0.1:4223/node2"           
Removed peer
```

### **GET**, **POST** `/insert`

Insert is used to send a newly constructed bundle from this node instance. 

A *GET* request expects a hex encoded bundle as its raw query parameter.

```
$ curl http://127.0.0.1:3000/insert?9f88071a000200040082016e2f2f6e6f646538342f66696c65738201712f2f6e6f646536352f696e636f6d696e678201712f2f6e6f646536352f696e636f6d696e67821b000000a0c364f04f001a0036ee80850a020000448218200085010100004443414243ff
Sent 103 bytes
```


The *POST* request expects a valid CBOR encoded bundle as request body. 
This is the preferred method for sending larger bundles, unless the [websocket interface](#websocket-application-agent-interface) is used.

### **GET** `/endpoint?<ENDPOINT>`

Download raw bundle from the supplied endpoint. 
This can be either a local singleton endpoint, e.g., `mailbox`, or a group endpoint such as `dtn://global/~news`. 

Returns either the raw bundle bytes or the string `Nothing to receive`

```
$ curl http://127.0.0.1:3000/endpoint?incoming
Nothing to receive
```


### **GET** `/endpoint.hex?<ENDPOINT>`

Download hex encoded bundle from the supplied endpoint. 
This can be either a local singleton endpoint, e.g., `mailbox`, or a group endpoint such as `dtn://global/~news`. 


### **GET** `/cts`

Get a new, unique creation timestamp (`[dtntime, seqno]`).

```
$ curl http://127.0.0.1:3000/cts
[690468652541,0]
```

### **GET** `/delete?<BID>`

Delete a specific bundle from the local store.

```
$ curl http://127.0.0.1:3000/delete?dtn://node1/-734350088476-0
Deleted bundle dtn://node1/-734350088476-0
```

### *DEBUG ONLY* **GET** `/debug/rnd_bundle`

This is a debug helper that inserts a random bundle into the local bundle store.

### *DEBUG ONLY* **GET** `/debug/rnd_peer`

This is a debug helper that inserts a random, nonexistent peer into the list of known peers.


### **WEBSOCKET** `/ws`

This endpoint is used to connect to the websocket application agent. 
For further information, see the corresponding [section](#websocket-application-agent-interface)
 in this document.

## Public API

### **GET** `/download.hex?<BID>`

Download a specific bundle as a hex string.

```
$ curl http://127.0.0.1:3000/download.hex?dtn://node1-690467584244-0
9f88071a00020004008201702f2f6e6f6465332f696e636f6d696e678201672f2f6e6f6465318201672f2f6e6f646531821b000000a0c31338f4001a0036ee80850a020000448218200085010100004645746573740aff
```

### **GET** `/download?<BID>`

Download a specific bundle as raw bytes.

```
$ curl http://127.0.0.1:3000/download?dtn://node1-690467584244-0 -o /tmp/msg.bundle
  % Total    % Received % Xferd  Average Speed   Time    Time     Time  Current
                                 Dload  Upload   Total   Spent    Left  Speed
100    87  100    87    0     0  87000      0 --:--:-- --:--:-- --:--:-- 87000
```

### **POST** `/push`

A CBOR encoded bundle can be directly sent to this API endpoint via *POST* and is then internally handled as if it was received by a CLA. 
Thus, this is not for new bundles generated by this node, but for external nodes to push their bundles into the store of the local *dtnd* instance.

```
$ bp7 rnd -r > /tmp/bundle.cbor
dtn://node11/sms-700045217469-0
$ curl -v -X POST --data-binary "@/tmp/bundle.cbor" http://127.0.0.1:3000/push
Received 99 bytes
```

### **GET** `/status/nodeid`

Get the dtn node ID of the running instance of `dtnd`.

```
$ curl http://127.0.0.1:3000/status/nodeid
dtn://node1
```

### **GET** `/status/eids`

Get a list of all registered endpoints at the local *dtnd* instance.

```
$ curl http://127.0.0.1:3000/status/eids
[
  "dtn://node1",
  "dtn://node1/incoming"
]
```

### **GET** `/status/bundles`

Get a list of all bundles at the local *dtnd* instance.

```
$ curl http://127.0.0.1:3000/status/bundles
[
  "dtn://node1-690467584244-0"
]
```

### **GET** `/status/bundles/digest`

Get a hash digest of all bundles at the local *dtnd* instance.

```
$ curl http://127.0.0.1:3000/status/bundles/digest
39caaad825bbdc3c
```

### **GET** `/status/bundles/verbose`

Get a list of all bundles at the local *dtnd* instance including source and destination EID, creation timestamp and bundle size.

```
$ curl "http://127.0.0.1:3000/status/bundles/verbose"             
[
  "dtn://node1/ dtn://node3/incoming 710077652064 105",
  "dtn://node29/123456 dtn://node50/123456 710077677209 99",
  "dtn://node14/files dtn://node32/~news 710077688966 96"
]
```

### **GET** `/status/bundles/filtered?addr=<address_part_criteria>`

Get a list of BIDs from the local *dtnd* instance which contain the filter criteria.

```
$ curl "http://127.0.0.1:3002/status/bundles/filtered?addr=node50"
[
  "dtn://node29/123456-710077677209-0"
]
```

### **GET** `/status/bundles/filtered/digest?addr=<address_part_criteria>`

Get a hash digest of the bundles matching the filter criteria at the local *dtnd* instance.

```
$ curl http://127.0.0.1:3000/status/bundles/filtered/digest?addr=node1
c08e54f9a9bc98d5
```

### **GET** `/status/store`

Get a list of all bundles at the local *dtnd* instance, including their constraints.

```
curl http://127.0.0.1:3000/status/store
[
  "dtn://node1-690467584244-0 {ForwardPending}"
]
```

### **GET** `/status/peers`

Get a list of all currently known peers.

```
$ curl http://127.0.0.1:3000/status/peers
{
  "node2": {
    "eid": [
      1,
      "//node2"
    ],
    "addr": "127.0.0.1",
    "con_type": "Static",
    "period": null,
    "cla_list": [
      [
        "mtcp",
        4223
      ]
    ],
    "services": {},
    "last_contact": 1637152383
  }
}
```

### **GET** `/status/info`

Get some general statistics about the running *dtnd* instance.

```
$ curl http://127.0.0.1:3000/status/info
{
  "incoming": 0,
  "dups": 0,
  "outgoing": 0,
  "delivered": 0,
  "broken": 0
}
```

## WebSocket Application Agent Interface

The websocket interface for application agents is reachable under `/ws`. 
There are a few control commands that can be sent as text messages.

- `/node` - returns the node id of the local instance
- `/subscribe <endpoint>` - receive incoming bundles for this endpoint via the current websocket. *NOTE: the endpoint must be already registered to subscribe to it!*
- `/unsubscribe <endpoint>` - stop receiving bundles for the given endpoint on this websocket connection. *NOTE: They are still collected on the node itself unless the endpoint is also unregistered!*
- `/data` - put this websocket into [cbor data mode](#data-mode). 
- `/json` - put this websocket into [json mode](#json-mode). 
- `/bundle` - put this websocket into raw [bundle mode](#bundle-mode). 

Sending and receiving happens as binary data directly on the websocket in the specified mode.

Various examples on how to use this interface from various programming languages can be found under `examples/` in the root of the *dtn7-rs* source directory.

### Data Mode

Encoding and decoding of the bundles is handled on the server side. 
Simpler structs that are CBOR encoded are used for data exchange.
These lack access to data from other canonical blocks or some less used header fields.

Here the definitions in pseudocode:
```
struct WsRecvData {
    /// the bundle id
    bid: String,
    /// the source endpoint ID, e.g., dtn://node1/sms
    src: String,
    /// the destination endpoint ID, e.g., dtn://global/~news            
    dst: String,
    // the payload data itself
    data: ByteBuffer,
}

struct WsSendData {
    /// source with a dtn URI scheme, e.g. dtn://node1 or ipn://23.0
    src: String,
    /// destination with a dtn URI scheme, e.g. dtn://node1/sms or ipn://23.42/
    dst: String,
    /// turn on delivery notifications
    delivery_notification: bool,
    /// lifetime for bundle in milliseconds
    lifetime: u64,
    /// payload data    
    data: ByteBuffer,
}
```

These structs must then be CBOR encoded or decoded prior to their usage.

### JSON Mode

Encoding and decoding of the bundles is handled on the server side. 
Simpler structs that are JSON encoded are used for data exchange. 
These lack access to data from other canonical blocks or some less used header fields.

The structs are identical to the ones of the [cbor data mode](#data-mode).
Starting at version 0.18 of *dtnd*, `ByteBuffer` are all base64 encoded to preserve bandwidth compared to transferring raw byte arrays in JSON.

### Bundle Mode

In bundle mode, the raw CBOR encoded bundles are sent via the websocket. 
Thus, clients must decode themselves and properly generate bundles to send them via the *dtnd* instance.

