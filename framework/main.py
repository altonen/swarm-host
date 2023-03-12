from backend.substrate.node import Polkadot, NodeTemplate, InvalidConfiguration

import time
import logging

logging.basicConfig(
    level   = logging.DEBUG,
    format  = '%(asctime)s.%(msecs)03d %(levelname)s %(module)s - %(funcName)s: %(message)s',
    datefmt = '%Y-%m-%d %H:%M:%S',
)

p2p_port = 8000
rpc_port = 9000
nodes = []

for i in range(0, 5):
    try:
        nodes.append(
            NodeTemplate()\
                .with_p2p_port(p2p_port)\
                .with_rpc_port(rpc_port)\
                .with_base_path(tmp = True)\
                .with_binary_path("/home/altonen/code/rust/substrate/target/release/node-template")\
                .build()
        )
        p2p_port += 1
        rpc_port += 1
    except InvalidConfiguration:
        print("invalid node configuration")

time.sleep(30)
