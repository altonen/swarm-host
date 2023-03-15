from backend.substrate.node import Polkadot, NodeTemplate, InvalidConfiguration
from notification import TransactionFilter

node = NodeTemplate()\
    .with_p2p_port(0 + 7000)\
    .with_rpc_port(0 + 8000)\
    .with_ws_port(0 + 9944)\
    .with_profile("alice")\
    .with_force_authoring()\
    .with_chain_spec(dev = True)\
    .with_base_path(tmp = True)\
    .with_reserved_peer("/ip6/::1/tcp/8888/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp")\
    .with_binary_path("/home/altonen/code/rust/substrate/target/release/node-template")\
    .build()

# context = base64.b64encode(node.get_metadata()).decode('utf-8')

filter = TransactionFilter(0, node.get_metadata())
json = filter.to_json()
# filter = TransactionFilter(0, node.get_metadata())
