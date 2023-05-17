from swarm_host.swarm_host import SwarmHost

import os

class SubstrateSwarmHost(SwarmHost):
    def __init__(self, rpc_port, genesis_hash):
        super().__init__(rpc_port, "substrate", genesis_hash)

    """
        Create new interface and initialize default handlers for:
          - `/sup/block-announces/1`
          - `/sup/sync/2`
    """
    def create_interface(
        self,
        port,
        preinit = None,
        context = None,
        block_announce_filter = None,
        sync_filter = None
    ):
        if "PYTHONPATH" not in os.environ:
            raise "`PYTHONPATH` must be defined"

        swarm_host_path = os.environ["PYTHONPATH"]
        if swarm_host_path[:-1] != "/":
            swarm_host_path += "/"

        if preinit is None:
            preinit = open(swarm_host_path + "swarm_host/backend/substrate/filters/preinit.py").read()
        if context is None:
            context = open(swarm_host_path + "swarm_host/backend/substrate/filters/context.py").read()
        self.id = super().create_interface(port, context, preinit)

        # install block announces filter
        filter = open(swarm_host_path + "swarm_host/backend/substrate/filters/block-announces-1.py").read()
        super().install_notification_filter(
            self.id,
            "/sup/block-announces/1",
            filter,
        )

        # install sync filter
        filter = open(swarm_host_path + "swarm_host/backend/substrate/filters/sup-sync-2.py").read()
        super().install_request_response_filter(
            self.id,
            "/sup/sync/2",
            filter,
        )
