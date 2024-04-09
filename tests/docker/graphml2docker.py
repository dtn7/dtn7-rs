#!/usr/bin/env python3

import os
import sys
import networkx as nx

template_global = """
version: '3.8'
name: '{}'

"""
template_service = """
  {}:
    hostname: {}
    container_name: {}
    image: {}
    cap_add:
      - NET_ADMIN
    networks: {}
    environment: {}
    privileged: true
    command: -C tcp

"""

template_service_net = """
      {}:
        ipv4_address: {}
"""

template_service_env = """
      - {}={}"""

template_network = """
  {}:
    driver: bridge
    ipam:
      config:
        - subnet: {}
"""


def generate_topology(scenario: str, g: nx.Graph, image="gh0st42/dtn7:bookworm"):
    g = g.to_undirected()
    print("Generating topology for scenario %s." % scenario, file=sys.stderr)
    outfile = template_global.format(scenario)

    services = "services:"
    networks = "networks:"

    cnt = 1
    hub_nets = {}
    net_map = {}
    for node in g.nodes:
        node_name = node.lower()
        if node_name.startswith("net_") or node_name.startswith("subnet_"):
            subnet = "10.%d.0" % cnt
            net_name = node_name
            net_map[net_name] = subnet
            networks += template_network.format(net_name, "%s.0/24" % subnet)
            hub_nets[node_name] = 2
            cnt += 1

    for net in g.edges:
        net = tuple(sorted(net))
        if (
            net[0].lower().startswith("net_")
            or net[0].lower().startswith("subnet_")
            or net[1].lower().startswith("net_")
            or net[1].lower().startswith("subnet_")
        ):
            continue
        net_name = "subnet_%s_%s" % (net[0].lower(), net[1].lower())
        subnet = "172.33.%d" % cnt
        networks += template_network.format(net_name, "%s.0/24" % subnet)
        net_map[net] = subnet
        cnt += 1

    for node, data in g.nodes(data=True):
        node_name = node.lower()
        if node_name.startswith("net_") or node_name.startswith("subnet_"):
            continue
        networks_out = ""
        for e in g.edges(node):
            if e[0] in hub_nets:
                node_ip = "%s.%d" % (net_map[e[0]], hub_nets[e[0]])
                networks_out += template_service_net.format(e[0], node_ip)
                hub_nets[e[0]] += 1
            elif e[1] in hub_nets:
                node_ip = "%s.%d" % (net_map[e[1]], hub_nets[e[1]])
                networks_out += template_service_net.format(e[1], node_ip)
                hub_nets[e[1]] += 1
            else:
                e = tuple(sorted(e))
                net_name = "subnet_%s_%s" % (e[0], e[1])
                if node == e[0]:
                    node_ip = "%s.%d" % (net_map[e], 2)
                else:
                    node_ip = "%s.%d" % (net_map[e], 3)
                networks_out += template_service_net.format(net_name, node_ip)
        env = ""
        for k, v in data.items():
            if k == "name":
                continue
            env += template_service_env.format(str.upper(k), v)
        # check if node name is a number
        if node.isdigit():
            env += template_service_env.format("NODE_ID", node)
        this_service = template_service.format(
            node, node.lower(), node.lower(), image, networks_out, env
        )
        services += this_service
    outfile += services

    outfile += networks
    print(outfile)


image = "gh0st42/dtn7:bookworm"
if len(sys.argv) < 2:
    print("Usage: %s <graphml_file> [image]" % sys.argv[0])
    sys.exit(1)

if len(sys.argv) > 2:
    image = sys.argv[2]


g = nx.read_graphml(sys.argv[1])

label_map = {}
for node, data in g.nodes(data=True):
    if "name" in data.keys():
        # print(data["name"])
        label_map[node] = data["name"]
        if node.isdigit():
            data["NODE_ID"] = node

if label_map != {}:
    nx.relabel_nodes(g, label_map, copy=False)

base_name = os.path.basename(sys.argv[1]).rsplit(".", 1)[0]
generate_topology(base_name, g, image)
