from swarm_host.backend.substrate.swarm_host import SubstrateSwarmHost
from swarm_host.backend.substrate.node import SubstrateNode
from swarm_host.filter import NotificationFilter

import time
import datetime

# start `swarm-host` executable with Substrate backend enabled and pass the genesis
# hash of the chain that the Substrate nodes will be running on.
swarm_host = SubstrateSwarmHost(
    rpc_port = 8884,
    genesis_hash = "0xb369c994b9cae41b25cabb76cae4b442ebcfbbb23596e2f9c2ee07f053512652"
)

# after running for 2 minutes, stop forwarding block announcements
class CustomBlockAnnounceFilter(NotificationFilter):
    def inject_notification(ctx, peer, protocol, notification):
        import swarm_host.backend.substrate.filters.block_announces_1
        import datetime

        if (datetime.datetime.now() - ctx.started).total_seconds() > 2 * 60:
            return
        return block_announce_1.inject_notification(ctx, peer, protocol, notification)

# spawn 10 interfaces and install default `preinit`, `context` and `/sup/sync/2` filters
# and a custom `/sup/block-announces/1` filter.
interfaces = []
for i in range(0, 10):
    interfaces.append(swarm_host.create_interface(0, block_announce_filter = CustomBlockAnnounceFilter().export()))

# spawn 3 Substrate nodes, one validator and two full nodes
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

for i in range(0, 2):
    nodes.append(SubstrateNode([
        "--in-peers", "3",
        "--out-peers", "2",
        "--port", "0",
        "--chain=dev",
        "--tmp",
    ])) 

# run the test for 5 minutes
time.sleep(5 * 60)
