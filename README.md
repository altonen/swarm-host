# `swarm-host`

`swarm-host` is a protocol simulation and traffic flow analysis tool for *peer-to-peer (P2P)* programs. It specifies a trait the application must implement which defines how its network protocols work and exposes a Python framework for programming said protocols. It also exposes a simple heuristics backend which provides visualization of the network traffic.

`swarm-host` allows running protocol simulations against live nodes of the P2P network and provides the ability to analyze the network behavior and traffic flow patterns during the simulation. It can be used to evaluate how, e.g., functional changes to protocol implementations or choices made in peer connections affect the security and bandwidth usage of the network.

`swarm-host` works by spawning Sybil nodes (called interfaces) in the network that masquerade as normal P2P nodes which honest nodes can connect to. The traffic flow in these interfaces is programmable in Python, allowing a wide range of simulations to be performed. The protocols can be programmed by installing a filter which is called each time a network event, such as an incoming peer or a block request, is received and the filter decides what it wants to do with that event. It can, e.g., decide to drop the received message, delay or modify it, or only forward it to some of its connected peers. The filters can also be tweaked during runtime and reinstalled if such a need exists.

In addition to being generic over the network protocol, `swarm-host` is also generic over the filter executor so if Python is not suitable for whatever reason, another executor can be implemented provided it can do the necessary conversions for the types the backend defines and that it can be called from Rust.

![](https://i.imgur.com/rWB0AvL.png)

**Figure 1 depicts a high-level architecture of `swarm-host` and an example testing scenario with 5:2 majority for honest nodes**

Following Python code shows how the testing scenario described above can be implemented for Substrate with a custom filter for the `/block-announces/1` protocol: 

```python
from swarm_host.swarm_host import SwarmHost
from swarm_host.filter import NotificationFilter
from swarm_host.backend.substrate.node import SubstrateNode

import time

swarm_host = SwarmHost(
    rpc_port = 8888,
    backend = "substrate",
    genesis_hash = "0x36d171b4279dc05f16e8960afd9ae9e28cd620b610b84d6374949c099231c585"
)

preinit = open("swarm_host/backend/substrate/filters/preinit.py").read()
context = open("swarm_host/backend/substrate/filters/context.py").read()

id1 = swarm_host.create_interface(6666, context, preinit)
id2 = swarm_host.create_interface(7777, context, preinit)

swarm_host.link_interface(id1, id2)

# create custom filter for the `/block-announces/1` protocol
# which stops forwarding block announcements after block 10
class CustomBlockAnnounceFilter(NotificationFilter):
    def inject_notification(ctx, peer, protocol, notification):
        from swarm_host.backend.substrate.primitives.block_announce import BlockAnnounce
        from swarm_host.backend.substrate.primitives.block_announce import init_runtime_config

        if ctx.runtime_config is None:
            ctx.runtime_config = init_runtime_config()

        try:
            block_announce = BlockAnnounce(ctx.runtime_config, bytes(notification))
        except Exception as e:
            print("failed to decode block announce:", e)
            return

        number = block_announce.number()
        hash = block_announce.hash(ctx.runtime_config)
        ctx.peers[peer].known_blocks.add(hash)

        # stop forwarding block announcements after block 10
        if number > 10:
            return

        forward_table = []
        for peer in ctx.peers:
            if hash not in ctx.peers[peer].known_blocks:
                forward_table.append(peer)

        if len(forward_table) != 0:
            ctx.forward_notification(protocol, forward_table, notification)

swarm_host.install_notification_filter(
    id1,
    "/sup/block-announces/1",
    CustomBlockAnnounceFilter().export()
)
swarm_host.install_notification_filter(
    id2,
    "/sup/block-announces/1",
    CustomBlockAnnounceFilter().export()
)

# start Substrate nodes
nodes = []
nodes.append(SubstrateNode([
    "--in-peers", "3",
    "--out-peers", "2",
    "--port", "0",
    "--chain=dev",
    "--alice",
    "--force-authoring",
    "--tmp",
]))

for i in range(0, 4):
    nodes.append(SubstrateNode([
        "--in-peers", "3",
        "--out-peers", "2",
        "--port", "0",
        "--chain=dev",
        "--tmp",
    ])) 

time.sleep(100)

```

Alternatively you can start `swarm-host` as a binary:

```sh
export PYTHONPATH=<path to swarm-host/framework>
cargo run -- --rpc-port 8888 substrate --genesis-hash 0x36d171b4279dc05f16e8960afd9ae9e28cd620b610b84d6374949c099231c585
```

and write the Python testing code without staring the `SwarmHost` object. This way you need to send commands to the running `swarm-host` binary over RPC. Calling conventions are documented in `src/rpc.rs` and `framework/swarm_host/swarm_host.py`.