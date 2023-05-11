from backend.substrate.node import Polkadot, NodeTemplate, InvalidConfiguration
from utils.swarm_host import SwarmHost

from jsonrpcclient import parse, request
import requests
import base64
import logging
import time
import inspect

logging.basicConfig(
    level   = logging.DEBUG,
    format  = '%(asctime)s.%(msecs)03d %(levelname)s %(module)s - %(funcName)s: %(message)s',
    datefmt = '%Y-%m-%d %H:%M:%S',
)

host = SwarmHost(8884, "substrate")
rpc_address = "127.0.0.1:4444"

nodes = []
nodes.append(NodeTemplate()\
    .with_p2p_port(8888)\
    .with_prometheus_port(9100)\
    .with_node_key("0000000000000000000000000000000000000000000000000000000000000001")\
    .with_chain_spec(dev = True)\
    .with_base_path(tmp = True)\
    .with_binary_path("/home/altonen/node-template")\
    .build()
)

time.sleep(1)

nodes.append(NodeTemplate()\
    .with_p2p_port(7777)
    .with_prometheus_port(9101)\
    .with_profile("alice")\
    .with_force_authoring()\
    .with_mdns(False)\
    .with_bootnode("/ip6/::1/tcp/8888/ws/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp")
    .with_chain_spec(dev = True)\
    .with_base_path(tmp = True)\
    .with_binary_path("/home/altonen/node-template")\
    .build()
)

for i in range(0, 10):
    nodes.append(NodeTemplate()\
        .with_p2p_port(6666 + i)\
        .with_prometheus_port(9102 + i)\
        .with_chain_spec(dev = True)\
        .with_mdns(False)\
        .with_bootnode("/ip6/::1/tcp/8888/ws/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp")
        .with_base_path(tmp = True)\
        .with_binary_path("/home/altonen/node-template")\
        .build()
    )

time.sleep(3 * 60)

context_filter = open("context.py").read()
block_filter = open("block-announce-filter.py").read()
sync_filter = open("sup-sync-2.py").read()
preinit_filter = open("preinit.py").read()

interfaces = []

for i in range(0, 40):
    iface_id = host.create_interface(rpc_address, context_filter, preinit = preinit_filter)
    host.install_notification_filter(iface_id, "/sup/block-announces/1", block_filter)
    host.install_request_response_filter(iface_id, "/sup/sync/2", sync_filter)

while True:
    time.sleep(200)
