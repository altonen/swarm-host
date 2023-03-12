from backend.substrate.node import Polkadot, NodeTemplate, InvalidConfiguration

import time
import logging

logging.basicConfig(
    level   = logging.DEBUG,
    format  = '%(asctime)s.%(msecs)03d %(levelname)s %(module)s - %(funcName)s: %(message)s',
    datefmt = '%Y-%m-%d %H:%M:%S',
)

nodes = []
for i in range(0, 1):
    try:
        nodes.append(
            NodeTemplate()\
                .with_p2p_port(i + 7000)\
                .with_rpc_port(i + 8000)\
                .with_ws_port(i + 9000)\
                .with_base_path(tmp = True)\
                .with_binary_path("/home/altonen/code/rust/substrate/target/release/node-template")\
                .build()
        )
    except InvalidConfiguration:
        print("invalid node configuration")

time.sleep(30)
