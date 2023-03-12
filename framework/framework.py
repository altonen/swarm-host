from backend.substrate.node import Polkadot, NodeTemplate, InvalidConfiguration

import time
import logging

logging.basicConfig(
    level   = logging.DEBUG,
    format  = '%(asctime)s.%(msecs)03d %(levelname)s %(module)s - %(funcName)s: %(message)s',
    datefmt = '%Y-%m-%d %H:%M:%S',
)

# TODO: what needs to happen:
#  1. start swarm-host with one interface
#  2. start two substrate nodes and connect them to the interface (`--reserved-only`)
#  3. verify that tx protocol is opened
#  4. install handler for tx protocol
#  5. send extrinsic to the validator node
#  6. verify that the extrinsic is received by swarm-host
#  7. pass extrinsic to the handler and verify it can be decoded

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
    .with_p2p_port(0 + 7000)\
    .with_rpc_port(0 + 8000)\
    .with_ws_port(0 + 9944)\
    .with_chain_spec(dev = True)\
    .with_base_path(tmp = True)\
    .with_binary_path("/home/altonen/code/rust/substrate/target/release/node-template")\
    .build()
)

