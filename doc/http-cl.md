Simple HTTP Convergence Layer
=============================

As `dtn7-rs` provides a simple HTTP API for its CLI helpers this interface can also be used to exchange bundles with other nodes.

Each `dtn7-rs` node provides a `/push` endpoint that accepts RFC 9171 bundles with `POST` requests.
Only one bundle per request is supported; thus, some overhead for HTTP connection management is involved.

One can also use tools such as `curl` to push bundles into a running `dtnd` instance.

```
$ bp7 rnd -r > /tmp/bundle.cbor
dtn://node11/sms-700045217469-0
$ curl -X POST --data-binary "@/tmp/bundle.cbor" http://127.0.0.1:3000/push
Received 99 bytes
```
