from swarm_host.swarm_host import SwarmHost

class SubstrateSwarmHost(SwarmHost):
    def __init__(self, rpc_port, genesis_hash):
        super().__init__(rpc_port, "substrate", genesis_hash)

    """
        Create new interface and initialize default handlers for:
          - `/sup/block-announces/1`
          - `/sup/sync/2`
    """
    def create_interface(self, port, preinit = None, context = None):
        if preinit is None:
            preinit = open("swarm_host/backend/substrate/filters/preinit.py").read()
        if context is None:
            context = open("swarm_host/backend/substrate/filters/context.py").read()
        self.id = super().create_interface(port, context, preinit)

        # install block announces filter
        filter = open("swarm_host/backend/substrate/filters/block-announces-1.py").read()
        super().install_notification_filter(
            self.id,
            "/sup/block-announces/1",
            filter,
        )

        # install sync filter
        filter = open("swarm_host/backend/substrate/filters/sup-sync-2.py").read()
        super().install_request_response_filter(
            self.id,
            "/sup/sync/2",
            filter,
        )
