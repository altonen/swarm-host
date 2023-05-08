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

time.sleep(1)

context_filter = open("context.py").read()
block_filter = open("block-announce-filter.py").read()
sync_filter = open("sup-sync-2.py").read()
preinit_filter = open("preinit.py").read()

interfaces = []

for i in range(0, 10):
    iface_id = host.create_interface(rpc_address, context_filter, preinit = preinit_filter)
    host.install_notification_filter(iface_id, "/sup/block-announces/1", block_filter)
    host.install_request_response_filter(iface_id, "/sup/sync/2", sync_filter)

time.sleep(1)

nodes = []
nodes.append(NodeTemplate()\
    .with_p2p_port(0 + 7000)\
    .with_rpc_port(0 + 8000)\
    .with_ws_port(0 + 9944)\
    .with_profile("alice")\
    .with_force_authoring()\
    .with_chain_spec(dev = True)\
    .with_base_path(tmp = True)\
    .with_binary_path("/home/altonen/code/rust/substrate/target/release/node-template")\
    .build()
)

nodes.append(NodeTemplate()\
    .with_p2p_port(1 + 7000)\
    .with_rpc_port(1 + 8000)\
    .with_ws_port(1 + 9944)\
    .with_chain_spec(dev = True)\
    .with_base_path(tmp = True)\
    .with_binary_path("/home/altonen/code/rust/substrate/target/release/node-template")\
    .build()
)

nodes.append(NodeTemplate()\
    .with_p2p_port(2 + 7000)\
    .with_rpc_port(2 + 8000)\
    .with_ws_port(2 + 9944)\
    .with_chain_spec(dev = True)\
    .with_base_path(tmp = True)\
    .with_binary_path("/home/altonen/code/rust/substrate/target/release/node-template")\
    .build()
)

nodes.append(NodeTemplate()\
    .with_p2p_port(3 + 7000)\
    .with_rpc_port(3 + 8000)\
    .with_ws_port(3 + 9944)\
    .with_chain_spec(dev = True)\
    .with_base_path(tmp = True)\
    .with_binary_path("/home/altonen/code/rust/substrate/target/release/node-template")\
    .build()
)

while True:
    time.sleep(200)
