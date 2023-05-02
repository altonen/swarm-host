
# TODO: what needs to happen here:
# TODO: 1. 

from scalecodec.base import RuntimeConfiguration, ScaleBytes
from scalecodec.type_registry import load_type_registry_preset
import json

class NotificationFilter():
    """
        Initialize filter.
    """
    def __init__(self, interface, protocol, context):
        self.interface = interface
        self.protocol = protocol
        self.context = context

    """
        Initialize filter context from JSON.
    """
    def initialize_context(context):
        self.runtime_config = RuntimeConfiguration()
        self.runtime_config.update_type_registry(load_type_registry_preset(name = "core"))
        self.metadata = RuntimeConfiguration().create_scale_object(
            'MetadataVersioned', data = ScaleBytes(context)
        )
        self.metadata.decode()
        self.runtime_config.add_portable_registry(self.metadata)
        pass

    """
        Execute custom filter for a received notification.
    """
    def inject_notification(
        context,
        src_interface,
        src_peer,
        dst_interface,
        dst_peer,
        notification
    ):
        pass

class SubstrateNotificationFilter(NotificationFilter):
    def __init__(self, interface, protocol, context):
        super().__init__(interface, protocol, context)

class TransactionFilter(SubstrateNotificationFilter):
    def __init__(self, interface, context):
        super().__init__(interface, "/sup/transactions/1", context)

    def inject_notification(
        context,
        src_interface,
        src_peer,
        dst_interface,
        dst_peer,
        notification
    ):
        pass
