
def spawn_interfaces():
    # TODO: spawn n sybil nodes and connect them to `swarm-host`

# TODO: is this needed?
def spawn_nodes():
    # TODO: spawn n sybil nodes and connect them to `swarm-host`

def spwan_network():
    # TODO: spawn a siloed network for honest nodes and connect those nodes together

def setup():
    self.spawn_interfaces()
    self.spawn_nodes()

def main():
    # TODO: describe some attack here
    #
    # TODO: requirements for the system:
    #  - ability to connect one or more honest, and siloed networks together through `swarm-host`
    #  - ability to join a sybil network with an honest network together through `swarm-host`
    #  - ability to control the flow of packets between networks
    #     - dropping all packets of a node
    #     - dropping percentage of packets of a node
    #     - dropping packets of certain type
    #     - dropping packets destined to some node
    #     - any combination of these
    #  - ability to masquerade external IPs
