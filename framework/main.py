from backend.substrate.node import Polkadot, NodeTemplate
import logging

logging.basicConfig(
    level   = logging.INFO,
    format  = '%(asctime)s.%(msecs)03d %(levelname)s %(module)s - %(funcName)s: %(message)s',
    datefmt = '%Y-%m-%d %H:%M:%S',
)

node = NodeTemplate()\
        .with_p2p_port(8888)\
        .with_rpc_port(8889)\
        .with_binary_path("/home/altonen/code/rust/substrate/target/release/node-template")\
        .build()

node.submit_extrinsic("hello, world")
