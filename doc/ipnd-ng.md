IPND-ng
===

This document describes in a minimalistic style how to implement a new [IPND](https://datatracker.ietf.org/doc/draft-irtf-dtnrg-ipnd/) variant for the bundle protocol version 7 ([RFC9171](https://datatracker.ietf.org/doc/html/rfc9171)), using [CBOR](https://cbor.io) to encode and decode beacons. 
This document condenses the thesis *Implementing and evaluating the IP Neighbor Discovery Protocol for Disruption Tolerant Networks* of Christian Schmidt. 
Except a minor type clarification (values of services-map), it describes exactly the beacon implementation contained in the thesis.

## Beacon Structure in [CDDL](https://datatracker.ietf.org/doc/html/rfc8610) Format

***Important note:** all fields are unnamed and the default composition type is *array*.
The only exception is the services field, where the composition type is *map*.*

Details about the data contained in the fields are given in the descriptions below.
```
BEACON = [
    version: uint
    flags: uint
    beacon_sequence_number: uint
    ? endpoint_identifier: EID
    ? service_block: SERVICE_BLOCK
    ? beacon_period: uint
]

EID = [
    scheme: uint
    specific_part: text
]

SERVICE_BLOCK = [
    convergence_layer_adapters: [* CLA]
    services: {* uint => bytes}
]

CLA = [
    type: text
    port: uint
]
```

| Field                          | Description                                                                                                                                                                                                                |
|--------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| version (8bit)                 | IPND version, matches bundle protocol version and therefore should be 0x07 currently, increases with new bp versions                                                                                                       |
| flags (8bit)                   | 0 (endpoint_identifier present), 1 (service_block present), 2 (beacon_period present), 3-7 (reserved)                                                                                                                      |
| beacon_sequence_number (32bit) | initially 0, incremented by one on each outgoing beacon to a specific IP address, wrap-around may occur and should be considered in the implementation                                                                        |
| endpoint_identifier            | a node-id as defined in  [RFC9171]( https://datatracker.ietf.org/doc/html/rfc9171), scheme (0 for DTN, 1 for IPN), specific_part (example for DTN: "//my-node-1/")                                                        |
| convergence_layer_adapters     | list of tuples, type (a text to define the CLA type, example: "mtcp"), port (the port at which the CLA listens, example: 16162)                                                                                            |
| services                       | map of, key (number between 0-255), value (bytes, describing the service and its capabilities)                                                                                                                             |
| beacon_period                  | announcement of the outgoing beacon interval in seconds, might be set to 0 to announce the departure of a node from that network, if no custom timeout is defined, beacon_period * 2 can serve as a timeout approximation  |

## Distribution

- Beacons may be sent via any available network interface using a local broadcast address or by joining an IP multicast group.
- The used transport protocol is UDP, where the payload is the CBOR encoded beacon structure.
- The default port is *3003*.

## Unicast Reply Extension (Optional)

The `beacon_period` of multiple nodes may be the same, but, they are rarely aligned. 
In the case of a new node joining the network, the un-alignment may lead to initially uni-directional paths if the `beacon_period` is large. 
In other words: the new node might send a beacon upon joining a network, but other nodes, already in the network might wait for a new beacon cycle before announcing themselves.

Therefore, we define the following *OPTIONAL* mechanism:

#### Informally: 

If we receive a beacon from a new node, and it is not a unicast beacon, we send back our last beacon as a unicast reply. 
We need to mark this beacon as unicast, so the other node knows it **must not** send a unicast reply back to us. 
There are two options to extend the beacon. The first option is to use a reserved flag and the second option is to use a service. 
As user-defined services are expected, we use the second option. 
A beacon is marked as unicast if it contains the service *42* and the service-value is the binary string *b"1"*.

##### Procedure:

**Note:** we assume our beacon to be one in-memory structure that gets modified and serialized as necessary.

1. If a beacon from a new node is received, or the `beacon_sequence_number` does not add up, proceed with 2.
2. If `not(42 in services and services[42]==b"1")`, proceed with 3.
3. Set `services[42]=b"1"` in own beacon, do NOT increase the beacon_sequence_number, proceed with 4.
4. Send own beacon via unicast address to new node. Delete `services[42]`.










