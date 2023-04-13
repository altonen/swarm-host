from backend.substrate.node import Polkadot, NodeTemplate, InvalidConfiguration
from utils.swarm_host import SwarmHost

from jsonrpcclient import parse, request
import requests
import base64
import logging
import time

logging.basicConfig(
    level   = logging.DEBUG,
    format  = '%(asctime)s.%(msecs)03d %(levelname)s %(module)s - %(funcName)s: %(message)s',
    datefmt = '%Y-%m-%d %H:%M:%S',
)

# host = SwarmHost(8884, "substrate")

# time.sleep(1)

iface1_addr = "127.0.0.1:4444"
response = requests.post(
    "http://localhost:%d/" % (8884),
    json = request(
        "create_interface",
        params = [iface1_addr],
    )
)

filter = open("context.py").read()

response = requests.post(
    "http://localhost:%d/" % (8884),
    json = request(
        "initialize_filter",
        params = [0, filter, ""],
    )
)

# iface1_id = host.create_interface(iface1_addr)
# print("interface id '%d'" % iface1_id)


filter = open("block-announce-filter.py").read()
# context = base64.b64encode(nodes[0].get_metadata()).decode('utf-8')

response = requests.post(
    "http://localhost:%d/" % (8884),
    json = request(
        "install_notification_filter",
        params = [0, "/sup/block-announces/1", "", filter],
    )
)
# host.install_notification_filter(iface1_id, "/sup/transactions/1", context, filter)

print("filter installed")

time.sleep(3)

# TODO: launching new nodes has to be made easier (better defaults)
nodes = []
nodes.append(NodeTemplate()\
    .with_p2p_port(0 + 7000)\
    .with_rpc_port(0 + 8000)\
    .with_ws_port(0 + 9944)\
    .with_profile("alice")\
    .with_force_authoring()\
    .with_chain_spec(dev = True)\
    .with_base_path(tmp = True)\
    .with_reserved_peer("/ip6/::1/tcp/8888/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp")
    .with_binary_path("/home/altonen/code/rust/substrate/target/release/node-template")\
    .build()
)
nodes.append(NodeTemplate()\
    .with_p2p_port(1 + 7000)\
    .with_rpc_port(1 + 8000)\
    .with_ws_port(1 + 9944)\
    .with_chain_spec(dev = True)\
    .with_base_path(tmp = True)\
    .with_reserved_peer("/ip6/::1/tcp/8888/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp")
    .with_binary_path("/home/altonen/code/rust/substrate/target/release/node-template")\
    .build()
)

# filter = open("transaction-filter.py").read()
# context = base64.b64encode(nodes[0].get_metadata()).decode('utf-8')

# response = requests.post(
#     "http://localhost:%d/" % (8884),
#     json = request(
#         "install_notification_filter",
#         params = [0, "/sup/transactions/1", context, filter],
#     )
# )
# # host.install_notification_filter(iface1_id, "/sup/transactions/1", context, filter)

# nodes[0].submit_extrinsic(
#     call_module = 'Balances',
#     call_function = 'transfer',
#     call_params = {
#         'dest': '5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty',
#         'value': 1 * 10**15
#     }
# )

while True:
    time.sleep(200)
